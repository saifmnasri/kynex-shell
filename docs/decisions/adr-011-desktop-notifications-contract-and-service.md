# ADR-011: Desktop Notifications Specification Implementation, Internal Notifications1 IPC, and Untrusted Input Sanitization Architecture

## Status
Accepted (Stage N / Phase 5 Notifications)

## Context
Desktop notifications in Linux desktop environments are standardized by the Freedesktop.org Desktop Notifications Specification (verified version 1.2). The specification defines a standard D-Bus session bus interface (`org.freedesktop.Notifications` at `/org/freedesktop/Notifications`) through which desktop applications and command-line utilities (e.g. `notify-send`) display transient, asynchronous popup notifications and receive user action callbacks.

In Kynex Shell, notifications represent a high-risk security boundary:
1. Any unprivileged local process on the session bus can invoke `Notify`.
2. Attackers frequently leverage notifications for HTML/script injection, terminal escape sequence injection, Unicode bidirectional (bidi) spoofing, image decompression bombs, D-Bus method flooding, and notification action hijacking.
3. Multiple desktop environments or notification daemons (e.g. `gnome-shell`, `dunst`, `mako`) might coexist on a developer's bus.

We must define:
1. **Bus Acquisition Policy**: How `kynex-daemon` claims `org.freedesktop.Notifications`.
2. **Standard Contract & Capabilities**: Which spec version, methods, signals, hints, and capabilities are advertised.
3. **Internal Shell Interface**: How `kynex-daemon` communicates with `kynex-shell` via `org.kynexos.Daemon.Notifications1`.
4. **Architectural Separation**: Isolating core state, sanitization, validation, and timers into a pure-Rust library crate (`kynex-notifications`) free of Qt and D-Bus dependencies.

---

## Part 1: Well-Known Bus Name Acquisition Policy

### Problem
If `kynex-daemon` attempts to replace an existing notification daemon or queue indefinitely for the name `org.freedesktop.Notifications`, it can destabilize existing desktop sessions or crash when another daemon does not yield the name. Furthermore, in nested development or containerized environments, another desktop (such as GNOME Shell) may already own the bus name.

### Policy & Implementation
1. **Exclusive, Non-Replacement Acquisition**: When starting the notification service on the session bus, `kynex-daemon` requests `org.freedesktop.Notifications` using `RequestNameFlags::DO_NOT_QUEUE` without `REPLACE_EXISTING`.
2. **Fail-Safe Fallback**: If the name is already owned by another process (D-Bus reply `DBUS_REQUEST_NAME_REPLY_EXISTS`, returned as `Err(zbus::Error::NameTaken)`), `kynex-daemon` logs a clear warning:
   `"Name 'org.freedesktop.Notifications' is already owned by another service. Disabling standard notifications endpoint on this bus."`
   The daemon continues normal operation without panicking or exiting.
3. **Internal Interface Unaffected**: The internal shell interface `org.kynexos.Daemon.Notifications1` on `/org/kynexos/Daemon/Notifications` (under the owned name `org.kynexos.Daemon`) remains active and accessible regardless of whether the external well-known name was claimed.

---

## Part 2: Freedesktop Specification Compliance & Advertised Capabilities

### Server Information
- **Name**: `kynex-daemon`
- **Vendor**: `Kynex Project`
- **Version**: `0.1.0` (matching workspace version)
- **Spec Version**: `1.2` (verified against Freedesktop specification and installed system packages)

### Advertised Capabilities (`GetCapabilities`)
- `body`: Server displays body text.
- `actions`: Server supports interactive action buttons and default body invocation.
- **Excluded: `body-markup`**: Body markup is strictly stripped to plain text. We reject the attack surface of HTML/rich-text parsing; hence `body-markup` is NOT advertised.
- **Excluded: `persistence`**: Notifications are retained in an in-memory ring buffer of 200 items during the daemon's active lifecycle, but are not serialized to disk across daemon restarts or system reboots. Per owner decision, `persistence` is omitted until persistent on-disk storage and a full notification center UI are implemented.

### Supported Hints
- `urgency` (`y` byte): `0` = Low, `1` = Normal, `2` = Critical. Critical notifications persist until dismissed and bypass Do Not Disturb (DND).
- `category` (`s` string): Sanitized string (max 64 bytes).
- `desktop-entry` (`s` string): Validated against the `AppsIndex`. If resolved, the application identity is verified and the application's official display name is shown. Otherwise, the client-provided `app_name` is flagged as unverified.
- `image-path` (`s` string) & `app_icon` (`s` string): Resolved safely through `kynex-icons` in the shell; external untrusted file paths are blocked.
- `image-data` (`(iiibiiay)` struct): Strictly validated raw pixel data (max 256x256, 8 bits per sample, 3 or 4 channels, exact rowstride arithmetic, total payload <= 256 KiB).
- `transient` (`b` bool): If true, notification is not retained in history after dismissal/expiry.
- `resident` (`b` bool): If true, clicking an action does not immediately dismiss the notification popup.
- **Ignored Hints**: `sound-file`, `sound-name`, `suppress-sound`, `x`, `y`, and all vendor-specific hints are safely discarded.

---

## Part 3: Internal Interface `org.kynexos.Daemon.Notifications1`

To maintain clean separation between the freedesktop compatibility layer and the shell UI, `kynex-daemon` exposes an internal interface tailored for `kynex-shell`:

- **Service Name**: `org.kynexos.Daemon`
- **Object Path**: `/org/kynexos/Daemon/Notifications`
- **Interface**: `org.kynexos.Daemon.Notifications1`

### Methods
- `Dismiss(id: u) -> ()`: Dismisses notification `id` (emits `NotificationClosed(id, 2)` to original sender).
- `InvokeAction(id: u, key: s) -> ()`: Invokes action `key` on notification `id` (emits `ActionInvoked(id, key)` exclusively to the original sender).
- `GetHistory() -> (a(ussssayby))` : Returns array of up to 200 past notifications: `(id, app_name, app_icon, summary, body, actions, app_name_verified, urgency)`.

### Properties
- `DoNotDisturb` (`b`, read-write): When enabled, transient popups are suppressed (kept in history; critical urgency notifications still pop up).

### Signals
- `Shown(id: u, app_name: s, app_name_verified: b, app_icon: s, summary: s, body: s, actions: as, urgency: y, timeout_ms: i)`
- `Updated(id: u, app_name: s, app_name_verified: b, app_icon: s, summary: s, body: s, actions: as, urgency: y, timeout_ms: i)`
- `Closed(id: u, reason: u)`

---

## Part 4: Pure-Rust Core Crate Architecture (`kynex-notifications`)

All notification state management, rate limiting, markup sanitization, and expiry calculations are encapsulated in `crates/kynex-notifications`:
- **Zero Qt / C++ dependencies**
- **Zero D-Bus dependencies** (abstracts senders via opaque sender IDs e.g. `SenderId(String)`)
- **`#![forbid(unsafe_code)]`** with zero `unwrap()` or `expect()` in production paths.
- **Monotonic ID Generation**: Starts at 1, wraps safely from `u32::MAX` back to 1, skipping 0 (reserved for new notifications) and skipping any active/history collisions.
- **Sender Authorization**: `replaces_id` and `CloseNotification` succeed only if the caller's unique bus name matches the notification's original creator. Cross-sender replacement or hijacking is denied.
- **Unicast Action Invocation**: Action callbacks are emitted only to the original sender's unique connection name. If the sender disconnected, actions are discarded safely.
- **Single-Timer Event Scheduler**: Tracks the earliest active expiry deadline; timer resets only when earlier notifications arrive, avoiding polling and maintaining 0.00 idle wakeups.

