# ADR-013: System Tray StatusNotifierWatcher Implementation, Internal Tray1 IPC, and Pure-Rust Core Crate Architecture

## Status
Accepted (Stage O / Phase 5 Tray)

## Context
System tray indicators in modern Linux desktop environments adhere to the StatusNotifierItem (SNI) specification (standardized collaboratively by KDE and Freedesktop.org). SNI replaces legacy X11 XEmbed system tray specifications with a model-view architecture communicating via D-Bus session bus interfaces:
1. `StatusNotifierWatcher`: A central singleton service (registered as `org.kde.StatusNotifierWatcher` and `org.freedesktop.StatusNotifierWatcher` at object path `/StatusNotifierWatcher`) that registers status notifier items and visualization hosts, tracking active items and notifying hosts when items appear, update, or vanish.
2. `StatusNotifierItem`: Application-provided services exposing status, titles, icon pixmaps, icon names, and tooltips, responding to activation and context menu method calls.
3. `StatusNotifierHost`: Desktop visualization panels (such as `kynex-shell`) displaying tray icons and relaying user interactions.

In Kynex Shell, tray items represent an untrusted external security boundary:
- Any unprivileged application connected to the user's session bus can register tray items.
- Adversarial items may attempt service spoofing (registering another client's bus name to hijack interactions or cause DoS), denial of service via memory exhaustion (massive `IconPixmap` byte arrays, decompression bombs, or endless items), integer overflow exploits, terminal/UI injection via control characters or Unicode bidirectional overrides (U+202A..=U+202E, U+2066..=U+2069), markup injection, or D-Bus signal flooding.
- Applications may hang indefinitely on property calls, risking deadlocking the daemon or UI thread.

We must define:
1. **Bus Acquisition Policy**: How `kynex-daemon` acquires `org.kde.StatusNotifierWatcher` and `org.freedesktop.StatusNotifierWatcher`.
2. **Specification Compliance & Sender Authentication**: Enforcing strict ownership validation on `RegisterStatusNotifierItem`, item capacity limits, and lifecycle tracking via `NameOwnerChanged`.
3. **Internal Shell Interface**: `org.kynexos.Daemon.Tray1` contract between `kynex-daemon` and `kynex-shell`.
4. **Architectural Separation**: Isolating core state, sanitization, validation, and rate limiting into a pure-Rust library crate (`kynex-tray`) free of Qt and D-Bus dependencies.
5. **Context Menu Scope**: Direct delegation to the item's own `ContextMenu(x, y)` method; `dbusmenu` is explicitly out of scope and documented as a later slice.

---

## Part 1: Well-Known Bus Name Acquisition Policy

### Problem
In multi-desktop or nested development environments, another tray watcher (e.g. from KDE Plasma, GNOME Shell extensions, or standalone tray daemons) might already be active on the session bus. Attempting to queue or replace an existing owner could destabilize existing sessions or stall startup.

### Policy & Implementation
1. **Exclusive, Non-Replacement Acquisition**: When starting the tray subsystem on the session bus, `kynex-daemon` requests both `org.kde.StatusNotifierWatcher` and `org.freedesktop.StatusNotifierWatcher` using `RequestNameFlags::DO_NOT_QUEUE` without `REPLACE_EXISTING`.
2. **Fail-Safe Fallback**: If either name is already owned by another process (`RequestNameReply::Exists` or `zbus::Error::NameTaken`), `kynex-daemon` logs a clear warning:
   `"Name '<name>' is already owned by another service. Disabling standard watcher endpoint on this bus."`
   The daemon continues normal operation without panicking or exiting.
3. **Internal Interface Unaffected**: The internal shell interface `org.kynexos.Daemon.Tray1` on `/org/kynexos/Daemon/Tray` (under the owned name `org.kynexos.Daemon`) remains active and accessible.

---

## Part 2: Specification Conformance, Sender Authentication, and Lifecycle

### 1. Registration Argument Semantics & Sender Verification
When a client invokes `RegisterStatusNotifierItem(service: s)`:
- If `service` starts with `/` (object path format):
  - Sender is extracted directly from the caller's unique bus name (e.g. `:1.42`).
  - Object path is `service`.
- If `service` does not start with `/`:
  - If `service` contains a `/` separator (`<service_name><object_path>`), it is split into service name and object path.
  - Otherwise, `service_name = service` and `object_path = "/StatusNotifierItem"`.
- **Sender Verification Invariant**:
  - If `service_name` is a unique bus name (starts with `:`), it MUST match the caller's own unique bus name.
  - If `service_name` is a well-known name, `kynex-daemon` verifies that the caller's unique bus name is the current primary owner of that well-known name (via `GetNameOwner`).
  - **A client CANNOT register someone else's service name.** Any mismatch is rejected with `zbus::fdo::Error::AccessDenied`.

### 2. Capacity & Resource Limits
- Maximum items per sender: `<= 4` items. Excess registration attempts by the same sender are rejected with `LimitsExceeded`.
- Maximum items total: `<= 64` items across the entire session. Excess attempts are rejected with `LimitsExceeded`.

### 3. Lifecycle Tracking
- Items are tracked by `(unique_bus_name, object_path)`.
- When an application disconnects or exits, `NameOwnerChanged(name, old_owner, new_owner)` with an empty `new_owner` automatically unregisters all items owned by that sender.
- Explicit unregistration removes the item immediately.
- Upon unregistration, `StatusNotifierItemUnregistered(id)` is emitted on the watcher, and `ItemRemoved(id)` is emitted on `Tray1`.

---

## Part 3: Internal Interface `org.kynexos.Daemon.Tray1`

To maintain clean separation between the external watcher specification and shell presentation, `kynex-daemon` exposes an internal interface tailored for `kynex-shell`:

- **Service Name**: `org.kynexos.Daemon`
- **Object Path**: `/org/kynexos/Daemon/Tray`
- **Interface**: `org.kynexos.Daemon.Tray1`

### Methods
- `GetItems() -> (a(ssssssba(iiay)s))`: Returns list of currently active items with sanitized metadata and validated icon pixmaps.
- `Activate(id: s, x: i, y: i) -> ()`: Asynchronously calls `org.kde.StatusNotifierItem.Activate(x, y)` on the item's bus name and object path.
- `SecondaryActivate(id: s, x: i, y: i) -> ()`: Asynchronously calls `org.kde.StatusNotifierItem.SecondaryActivate(x, y)` on the item.
- `ContextMenu(id: s, x: i, y: i) -> ()`: Asynchronously calls `org.kde.StatusNotifierItem.ContextMenu(x, y)` on the item. The application is responsible for drawing its own context menu window.
- `Scroll(id: s, delta: i, orientation: s) -> ()`: Asynchronously calls `org.kde.StatusNotifierItem.Scroll(delta, orientation)` on the item.

### Signals
- `ItemAdded(item: TrayItemInfo)`: Emitted when a new tray item is successfully validated and added.
- `ItemChanged(item: TrayItemInfo)`: Emitted when an existing tray item's properties update.
- `ItemRemoved(id: s)`: Emitted when a tray item is unregistered or its owner disconnects.

---

## Part 4: Pure-Rust Core Crate Architecture (`kynex-tray`)

All registry logic, input sanitization, checked pixmap validation, and rate limiting are encapsulated in `crates/kynex-tray`:
- **`#![forbid(unsafe_code)]`** with zero `unwrap()`, zero `expect()`, and zero `unsafe`.
- **Zero Qt / C++ dependencies**.
- **Zero D-Bus dependencies** (abstracts senders via opaque sender IDs).
- **IconPixmap Validation**:
  - Checked arithmetic: `width > 0 && width <= 64`, `height > 0 && height <= 64`.
  - Byte length check: `width * height * 4` must match data buffer length exactly.
  - Size cap: Total payload must be `<= 64 KiB`.
  - ARGB to RGBA byte conversion: Network byte order ARGB32 converted to RGBA with tested bounds.
  - Lying dimensions, negative values, integer overflows, truncated buffers, and oversized payloads are strictly rejected.
- **IconName Filtering**:
  - Must conform to `kynex-icons` naming standard (`[a-zA-Z0-9._-]+` <= 128 chars).
  - Absolute paths (starting with `/`), traversal sequences (`..`), and path separators are strictly rejected.
- **String Sanitization**:
  - `Title`, `Id`, and tooltip `title` / `text` are stripped of ASCII control characters (0x00-0x1F, 0x7F) and Unicode bidi overrides/isolates (U+202A..=U+202E, U+2066..=U+2069).
  - HTML/XML markup tags are stripped to plain text.
  - Length caps: Id <= 64 chars, Title <= 128 chars, tooltip title <= 128 chars, tooltip text <= 512 chars.
- **Asynchronous Property Timeouts**:
  - All property reads from items are wrapped in a 2-second timeout (`PROPERTY_TIMEOUT_SECS = 2`). Hanging items can never stall the daemon.
- **Signal-Driven Refresh & Rate Limiting**:
  - Property updates are triggered by signals (`NewIcon`, `NewTitle`, `NewStatus`, `NewToolTip`, `NewAttentionIcon`, `PropertiesChanged`).
  - Updates are rate-limited to 10 refreshes/second per item, mitigating signal flood DoS.

