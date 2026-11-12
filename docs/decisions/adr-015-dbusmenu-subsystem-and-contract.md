# ADR-015: Host-Side D-Bus Menu (dbusmenu) Subsystem, IPC Contract, and Security Boundaries

## Status
Accepted (Stage P)

## Context
Under the StatusNotifierItem (SNI) specification, applications export menus in one of two ways:
1. Delegating directly to the item application via `ContextMenu(x, y)` to render a foreign popup window. On Wayland, this frequently fails or misplaces popups due to protocol security boundaries preventing unprivileged clients from positioning arbitrary top-level popup surfaces at specified screen coordinates (ADR-014).
2. Exporting an object implementing `com.canonical.dbusmenu` (or `org.freedesktop.DBusMenu`) at an object path specified by the `Menu` property of the StatusNotifierItem.

However, external client menu definitions are completely untrusted:
- An adversarial application can construct infinitely deep menu trees (causing recursion stack overflows), cycle graphs, millions of menu items (causing memory exhaustion / OOM), malformed variant types (causing deserialization crashes), huge string payloads (causing rendering stalls), or spoofed control/bidi characters.
- A malfunctioning application can hang on `GetLayout` or `AboutToShow` calls, stalling IPC threads.
- A malicious application could flood `LayoutUpdated` signals to starve the daemon CPU.
- Direct interaction between the QML shell UI and the untrusted external application's D-Bus object path violates the Kynex Shell security architecture: the shell must interact exclusively with internal `org.kynexos.Daemon` interfaces through validated Rust models.

## Decisions

### 1. Architectural Boundary Separation
- **Daemon (`kynex-daemon`)**: Talks `com.canonical.dbusmenu` to the item's Menu object path over the session bus. Performs all asynchronous IPC, 2-second timeout enforcement, signal monitoring, revision caching, rate limiting, and marshaling.
- **Pure-Rust Core (`kynex-dbusmenu`)**: Parses and validates the raw variant tree into a flat, sanitized structure with zero D-Bus and zero Qt dependencies (`#![forbid(unsafe_code)]`, GPL-3.0-or-later).
- **Internal Contract (`org.kynexos.Daemon.Tray1`)**: Exposes sanitized, bounded menu data to `kynex-shell`. The shell NEVER communicates directly with `com.canonical.dbusmenu`.
- **Shell UI (`kynex-shell` & `Kynex.Controls`)**: `TrayModel` provides thin glue over pure-Rust logic in `kynex-shell-core`. `KynexMenu` renders a token-driven popup menu inside a transparent full-screen overlay layer surface that handles outside clicks and Esc dismissal without blocking Qt.

### 2. Internal D-Bus API Extension on `org.kynexos.Daemon.Tray1`
The internal `Tray1` interface on `/org/kynexos/Daemon/Tray` is extended with:
- `GetMenu(item_id: s) -> (revision: u, items: a(iisbbuuiisb))`:
  Returns the cached or lazily fetched menu layout for `item_id`. `items` is a flat, ordered, sanitized list of menu nodes:
  - `id`: `i32` (menu item identifier)
  - `parent`: `i32` (parent item identifier, or 0 for top-level items)
  - `label`: `String` (sanitized label text)
  - `enabled`: `bool` (whether item is interactive)
  - `visible`: `bool` (whether item is visible)
  - `kind`: `u32` (0 = Normal, 1 = Separator)
  - `toggle`: `u32` (0 = None, 1 = Checkbox, 2 = Radio)
  - `toggle_state`: `i32` (-1 = Indeterminate/Unknown, 0 = Unchecked, 1 = Checked)
  - `icon_name`: `String` (sanitized theme icon name or empty)
  - `disposition`: `u32` (0 = Normal, 1 = Informative, 2 = Warning, 3 = Alert)
  - `shortcut`: `String` (sanitized accelerator string, e.g. "Ctrl+Q", or empty)
  - `has_submenu`: `bool` (true if node has visible children)
- `MenuAboutToShow(item_id: s, menu_item_id: i) -> (need_update: b)`:
  Invokes `AboutToShow(menu_item_id)` on the client's dbusmenu service. If `need_update` is true, daemon triggers an asynchronous, bounded re-fetch.
- `MenuEvent(item_id: s, menu_item_id: i, event: s, revision: u) -> ()`:
  Validates that `revision` equals the CURRENT cached layout revision, `menu_item_id` exists in that validated layout, is enabled, is not a separator, and `event` is strictly `"clicked"` or `"hovered"` before dispatching `Event` to the client. If `revision` is stale or mismatched, rejects with `org.freedesktop.DBus.Error.InvalidArgs`.
- Signal `MenuChanged(item_id: s, revision: u)`:
  Emitted when a menu's layout or properties are updated.

### 3. Hard Resource Bounds and Sanitization Rules
The pure-Rust parser enforces the following fail-closed limits:
- **Nesting depth**: `<= 6`. Deeper trees cause the entire layout to be rejected.
- **Total items**: `<= 500`. Excess item count causes the entire layout to be rejected.
- **Children per node**: `<= 100`. Excess children cause the entire layout to be rejected.
- **ID validation**: Root node is ID `0`. All other IDs must be non-negative integers (`>= 0`). Duplicates, cycles, or nodes that are their own ancestors cause the entire layout to be rejected.
- **Label sanitization**: Strips HTML/XML tags, drops ASCII controls (0x00-0x1F, 0x7F) and Unicode bidi overrides/isolates (U+202A-202E, U+2066-2069). Mnemonic underscores are parsed cleanly: `_File` becomes `File`, `__` becomes literal `_`. Capped at 256 characters.
- **Property type safety**: Missing or malformed property types are ignored and replaced with safe defaults without panic.
- **Strict mapping**: Toggle type strictly maps to None/Checkbox/Radio; toggle state strictly maps to Unchecked/Checked/Unknown; disposition strictly maps to Normal/Informative/Warning/Alert.
- **Icon names**: Must conform to Freedesktop icon naming rules (`[a-zA-Z0-9._-]`, <= 128 chars); raw file paths and slashes are rejected. No PNG/raster byte rendering (`icon-data` is ignored).
- **Visibility & Enabled**: Invisible items (`visible == false`) are dropped from the flat output; disabled items are retained with `enabled = false`.
- **Timeouts & Concurrency**: 2-second timeout per D-Bus call; max 8 concurrent fetches; per-item refresh rate limit of 10/s.
- **Lazy fetching & lifecycle**: Menus are fetched lazily when the shell requests them. Cached layouts are dropped when the item unregisters or its D-Bus bus owner disconnects.

