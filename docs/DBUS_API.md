# Kynex D-Bus API

Bus: session. Service name: `org.kynexos.Daemon`. Object path root: `/org/kynexos/Daemon`.

Rules: version every interface (`...1`), use signals for changes, never break an interface without bumping the version. Update this file in the same task as any API change. All inputs validated; sensitive methods check the caller.

## Interfaces
| Interface | Purpose | Status |
|---|---|---|
| `org.kynexos.Daemon.Core1` | daemon ping, version, and active service status | active |
| `org.kynexos.Daemon.Power1` | battery, power source, and charge metrics (wraps UPower) | active |
| `org.kynexos.Daemon.Network1` | wifi/ethernet state (wraps NetworkManager) | active |
| `org.kynexos.Daemon.WindowsFeed1` | private feeder interface for KWin script to report window state changes | active |
| `org.kynexos.Daemon.Windows1` | read-only window list and signals for the shell (Phase 3 Slice 1) | active |
| `org.kynexos.Daemon.ShellFeed1` | private feeder interface for KWin script to signal shell global shortcuts | active |
| `org.kynexos.Daemon.Shell1` | public shell events and toggle signals (Phase 4 Slice 2) | active |
| `org.kynexos.Daemon.Audio1` | volume, mute, devices (wraps PipeWire) | planned |
| `org.kynexos.Daemon.Apps1` | .desktop index, search, launch (Phase 4 Slice 1) | active |
| `org.kynexos.Daemon.Pins1` | pinned application management and persistence (Stage M / Phase 3) | active |
| `org.kynexos.Daemon.Search1` | global search providers (apps, files, calc, settings) | planned |
| `org.kynexos.Daemon.Session1` | lock, logout, shutdown (wraps logind) | planned |
| `org.kynexos.Daemon.Style1` | active style, list styles, switch, per-style KWin settings | planned |
| `org.freedesktop.Notifications` | desktop notification server (standard v1.2) | active |
| `org.kynexos.Daemon.Notifications1` | internal notification model interface for shell (Stage N / Phase 5) | active |
| `org.kde.StatusNotifierWatcher` | tray watcher service (standard SNI) | active |
| `org.freedesktop.StatusNotifierWatcher` | tray watcher service (freedesktop SNI alias) | active |
| `org.kynexos.Daemon.Tray1` | internal tray model and interaction interface for shell (Stage O / Phase 5) | active |

## Interface Details

### `org.kynexos.Daemon.Core1`
- Object path: `/org/kynexos/Daemon`
- Methods:
  - `Ping() -> (s)`: Returns "pong" verifying IPC roundtrip.
  - `Version() -> (s)`: Returns daemon software version.
  - `Status() -> ((stss))`: Returns struct of version, uptime in seconds, and active service names.
- Signals: none
- Properties: none
- Test commands:
  ```bash
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Core1 Ping
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Core1 Version
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Core1 Status
  ```

### `org.kynexos.Daemon.Power1`
- Object path: `/org/kynexos/Daemon`
- Methods: none
- Signals:
  - `org.freedesktop.DBus.Properties.PropertiesChanged`: Emitted when any power property changes.
- Properties:
  - `Present` (`b`, read-only): `true` if a battery device is detected and present; `false` otherwise.
  - `Percentage` (`d`, read-only): Battery charge level clamped between `0.0` and `100.0`. Returns `0.0` if not present.
  - `State` (`u`, read-only): Operational state (0=Unknown, 1=Charging, 2=Discharging, 3=Empty, 4=FullyCharged, 5=PendingCharge, 6=PendingDischarge).
  - `TimeToEmpty` (`x`, read-only): Estimated seconds remaining until battery is depleted; 0 if unknown or charging.
  - `TimeToFull` (`x`, read-only): Estimated seconds remaining until battery is fully charged; 0 if unknown or discharging.
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1 Present
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1 Percentage
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1 State
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1 TimeToEmpty
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1 TimeToFull
  ```

### `org.kynexos.Daemon.Network1`
- Object path: `/org/kynexos/Daemon/Network`
- Methods: none
- Signals:
  - `org.freedesktop.DBus.Properties.PropertiesChanged`: Emitted when any network property changes.
- Properties:
  - `Present` (`b`, read-only): `true` if network hardware/connectivity is managed and present; `false` otherwise.
  - `State` (`u`, read-only): Operational state (0=Disconnected, 1=Connecting, 2=Connected, 3=Unknown).
  - `Kind` (`u`, read-only): Network connection medium (0=None, 1=Wifi, 2=Ethernet, 3=Other).
  - `SignalStrength` (`u`, read-only): Wi-Fi signal quality percentage clamped between 0 and 100. Returns 0 for non-wireless or disconnected state.
  - `Ssid` (`s`, read-only): Sanitized SSID string for the active Wi-Fi connection. Capped at 32 bytes, stripped of control characters and Unicode bidi/override characters (U+202A-202E, U+2066-2069). Empty string if not connected to Wi-Fi.
- Privacy invariant: Never exposes IP addresses, BSSID, or MAC addresses.
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Network org.kynexos.Daemon.Network1
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon/Network org.kynexos.Daemon.Network1 Present
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon/Network org.kynexos.Daemon.Network1 State
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon/Network org.kynexos.Daemon.Network1 Kind
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon/Network org.kynexos.Daemon.Network1 SignalStrength
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon/Network org.kynexos.Daemon.Network1 Ssid
  ```

### `org.kynexos.Daemon.WindowsFeed1`
Private feeder interface invoked exclusively by the KWin window management script. Uses flat primitive arguments because KWin's `callDBus` does not support complex D-Bus container types (structs or dicts). Protected by `SenderPolicy`: rejects senders unless session UID matches and `/proc/<pid>/exe` is `kwin_wayland`.
- Object path: `/org/kynexos/Daemon/Windows`
- Methods:
  - `Reset() -> ()`: Clears all tracked windows. Invoked by KWin script upon initialization or recovery.
  - `Upsert(id: s, title: s, appId: s, minimized: b, active: b) -> ()`: Inserts or updates window metadata.
  - `Remove(id: s) -> ()`: Removes window with given ID.
  - `SetActive(id: s) -> ()`: Updates active focus to the specified window ID (empty string `""` clears active window).
- Signals: none
- Properties: none
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.WindowsFeed1
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.WindowsFeed1 Reset
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.WindowsFeed1 Upsert sssbb "win-1" "Terminal" "org.kde.konsole" false true
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.WindowsFeed1 SetActive s "win-1"
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.WindowsFeed1 Remove s "win-1"
  ```

### `org.kynexos.Daemon.Windows1`
Public window management interface for `kynex-shell` (and authorized session components). Exposes current windows, emits change signals with zero client polling, and provides window control actions.
- Object path: `/org/kynexos/Daemon/Windows`
- Methods:
  - `List() -> (a(sssbb))`: Returns an array of structs `(id, title, appId, minimized, active)` for all currently tracked normal windows.
  - `Activate(id: s) -> ()`: Activates (focuses and raises) the window matching `id`.
  - `Minimize(id: s) -> ()`: Minimizes the window matching `id`.
  - `Restore(id: s) -> ()`: Restores (un-minimizes) the window matching `id`.
  - `Close(id: s) -> ()`: Closes the window matching `id`.
- Caller Authorization & Security:
  - Caller must match the daemon's own session UID (verified via D-Bus `GetConnectionCredentials`).
  - Calls with unknown or non-existent IDs are rejected with `org.freedesktop.DBus.Error.InvalidArgs`.
  - Action invocations are rate-limited to 20 calls/second per client to protect the compositor from flooding.
- Accepted Residual Risk:
  - Any local process running under the same user UID that connects to the session bus can invoke window actions for tracked windows. In standard Linux desktop environments, the session bus is a shared boundary for the logged-in user. A stronger capability or per-client token delegation scheme is deferred.
- Signals:
  - `Added(id: s, title: s, appId: s, minimized: b, active: b)`: Emitted when a new window is tracked.
  - `Changed(id: s, title: s, appId: s, minimized: b, active: b)`: Emitted when title, appId, or minimized state changes.
  - `Removed(id: s)`: Emitted when a window is closed or untracked.
  - `ActiveChanged(id: s)`: Emitted when the active window changes (passes new active window ID, or `""` if none).
- Properties: none
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.Windows1
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.Windows1 List
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.Windows1 Activate s "win-1"
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.Windows1 Minimize s "win-1"
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.Windows1 Restore s "win-1"
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.Windows1 Close s "win-1"
  busctl --user monitor org.kynexos.Daemon
  ```

### `org.kynexos.Daemon.Apps1`
Application indexing, search, and launching service. Parses XDG Desktop Entry Specification `.desktop` files from `$XDG_DATA_DIRS/applications`, watches for directory changes with debouncing, provides prefix/word-start subsequence ranked search, and launches applications securely without a shell.
- Object path: `/org/kynexos/Daemon/Apps`
- Methods:
  - `GetApps() -> (a(sssssas))`: Returns an array of application entries: `(id, name, generic_name, comment, icon_name, categories)`.
  - `GetAppMapping() -> (a(sss))`: Returns an array of application mapping entries: `(id, startup_wm_class, icon_name)`. Used by shell for window-to-app matching and pinned app icon resolution without altering existing signatures.
  - `Search(query: s, limit: u) -> (a(sssssas))`: Returns ranked application entries matching `query` up to `limit` entries.
    - Limits: `query.len() <= 128` chars (rejects with `org.freedesktop.DBus.Error.InvalidArgs` if exceeded), `limit <= 50` (clamped to max 50).
  - `Launch(id: s) -> ()`: Spawns the application binary associated with desktop file `id`.
    - Authorization: Caller UID must match daemon session UID (`GetConnectionCredentials`).
    - Preconditions: Application `id` must exist in index (returns `InvalidArgs` if missing). If `Terminal=true` (`needs_terminal`), returns `Failed("Terminal applications are unsupported yet")`.
    - Rate limit: 5 calls/s daemon-wide (all callers share the same session UID). Rejects excess calls with `org.freedesktop.DBus.Error.LimitsExceeded`.
    - Execution environment: Direct binary execution (`execvp` / `tokio::process::Command`) without invoking `/bin/sh` or any shell interpreter; stdin connected to `/dev/null`; non-standard file descriptors closed; environment sanitized strictly to an allowlist (`HOME`, `PATH`, `XDG_*`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `LANG`, `LC_*`, `DISPLAY`).
    - Process lifetime: Detached process group (`setsid` / `process_group(0)`) so child survives daemon restarts; children are actively reaped via asynchronous waiter tasks per child without semaphore bottlenecks, preventing zombie processes while tracking active children with an atomic gauge. Surviving children are reparented to the session subreaper upon daemon restart.
- Signals:
  - `Changed()`: Emitted when `.desktop` files are added, modified, or removed across `$XDG_DATA_DIRS/applications` (debounced by 200ms).
- Properties: none
- Errors:
  - `org.freedesktop.DBus.Error.AccessDenied`: Caller UID mismatch.
  - `org.freedesktop.DBus.Error.InvalidArgs`: Query > 128 chars, non-existent `id`, or malformed parameters.
  - `org.freedesktop.DBus.Error.LimitsExceeded`: Launch rate limit exceeded (> 5 calls/s).
  - `org.freedesktop.DBus.Error.Failed`: Terminal execution not yet supported or process execution failure.
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Apps org.kynexos.Daemon.Apps1
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Apps org.kynexos.Daemon.Apps1 GetApps
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Apps org.kynexos.Daemon.Apps1 Search su "term" 10
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Apps org.kynexos.Daemon.Apps1 Launch s "org.kde.konsole.desktop"
  ```

### `org.kynexos.Daemon.Pins1`
Pinned application management and configuration persistence service. Tracks the ordered list of user-pinned desktop application IDs, persists state atomically to `$XDG_CONFIG_HOME/kynex/pinned.toml`, and notifies the session of pin modifications.
- Object path: `/org/kynexos/Daemon/Pins`
- Methods:
  - `Get() -> (as)`: Returns array of pinned desktop application IDs in display order.
  - `Pin(id: s) -> ()`: Pins desktop application `id`. If already pinned, preserves order or moves to end. ID must match `^[a-zA-Z0-9._-]+$` and length <= 128. Maximum pinned applications is clamped to 128.
  - `Unpin(id: s) -> ()`: Unpins desktop application `id`. No-op if `id` is not pinned.
  - `Move(id: s, index: u) -> ()`: Moves pinned application `id` to 0-based target `index`. Clamped to `pins.len() - 1` if index is out of bounds.
- Signals:
  - `Changed()`: Emitted whenever the pinned applications list changes.
- Persistence & Fault Tolerance:
  - Storage location: `$XDG_CONFIG_HOME/kynex/pinned.toml` (defaults to `~/.config/kynex/pinned.toml`).
  - Atomic persistence: Writes to a temporary file in the same directory (`.pinned.toml.tmp.<uuid>`) with mode `0600`, calls `sync_all()`, and atomically renames over the target path.
  - Hard caps: File size <= 64 KiB, <= 128 pins. Rejects pin additions beyond 128 entries.
  - Unknown application IDs: Kept in list across saves but flagged as missing in runtime models.
  - Corrupt file handling: If `pinned.toml` is malformed, oversized, or unparsable, the daemon safely renames the file to `pinned.toml.bad` and initializes with an empty pin list without panicking.
  - Mutation rate limiting: Global 10 mutations/s across all D-Bus connections. Excess calls rejected with `org.freedesktop.DBus.Error.LimitsExceeded`.
  - Caller authorization: Verifies caller UID matches daemon session UID via `GetConnectionCredentials`.
  - Residual Risk: All local processes running under the same user UID on the session bus can read and mutate pinned applications.
- Errors:
  - `org.freedesktop.DBus.Error.AccessDenied`: Caller UID mismatch.
  - `org.freedesktop.DBus.Error.InvalidArgs`: Empty `id`, invalid characters in `id`, or `id.len() > 128`.
  - `org.freedesktop.DBus.Error.LimitsExceeded`: Mutation rate limit exceeded (> 10/s) or max pins reached (128).
  - `org.freedesktop.DBus.Error.Failed`: Filesystem I/O or atomic rename failure.
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Pins org.kynexos.Daemon.Pins1
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Pins org.kynexos.Daemon.Pins1 Get
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Pins org.kynexos.Daemon.Pins1 Pin s "org.kde.konsole.desktop"
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Pins org.kynexos.Daemon.Pins1 Move su "org.kde.konsole.desktop" 0
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Pins org.kynexos.Daemon.Pins1 Unpin s "org.kde.konsole.desktop"
  ```

### `org.kynexos.Daemon.ShellFeed1`
Private feeder interface invoked exclusively by the KWin shortcut script to notify the daemon of global shell key shortcuts (e.g. Meta+Space). Protected by the exact same layered, fail-closed `SenderPolicy` as `WindowsFeed1`: verifies session UID matches daemon UID, unique connection bus name matches current `GetNameOwner("org.kde.KWin")`, and `/proc/<pid>/exe` equals `/usr/bin/kwin_wayland`.
- Object path: `/org/kynexos/Daemon/Shell`
- Methods:
  - `LauncherShortcut() -> ()`: Triggers launcher overlay visibility toggle. Authorized only for KWin compositor process.
- Signals: none
- Properties: none
- Threat Assessment:
  - Caller spoofing: Fail-closed verification prevents unprivileged user processes from posing as KWin.
  - Residual impact: Even in the event of an authorized toggle, the worst a spoofed or rogue toggle can do is open or close the launcher overlay on the desktop. It cannot run commands, exfiltrate data, or alter privileges.
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Shell org.kynexos.Daemon.ShellFeed1
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Shell org.kynexos.Daemon.ShellFeed1 LauncherShortcut
  ```

### `org.kynexos.Daemon.Shell1`
Public shell interface for desktop surfaces (`kynex-shell`). Emits signals for shell state and keyboard shortcut actions.
- Object path: `/org/kynexos/Daemon/Shell`
- Methods: none
- Signals:
  - `LauncherToggleRequested()`: Emitted when `LauncherShortcut()` is received from authorized KWin feeder. Signals `kynex-shell` to toggle the launcher overlay.
- Properties: none
- Threat Assessment:
  - Worst a spoofed toggle can do is open/close the launcher overlay.
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Shell org.kynexos.Daemon.Shell1
  busctl --user monitor org.kynexos.Daemon
  ```

### `org.freedesktop.Notifications`
Standard Freedesktop.org Desktop Notifications Specification v1.2 server. Implemented by `kynex-daemon` and bound to `/org/freedesktop/Notifications`. Acquired with non-replacement flags (`DO_NOT_QUEUE`, never replacing existing owners; disables standard endpoint gracefully if already claimed).
- Service name: `org.freedesktop.Notifications` (when acquired)
- Object path: `/org/freedesktop/Notifications`
- Methods:
  - `GetCapabilities() -> (as)`: Returns supported server capabilities: `["body", "actions"]`.
  - `GetServerInformation() -> (s, s, s, s)`: Returns name (`"kynex-daemon"`), vendor (`"Kynex Project"`), version (`"0.1.0"`), and spec_version (`"1.2"`).
  - `Notify(app_name: s, replaces_id: u, app_icon: s, summary: s, body: s, actions: as, hints: a{sv}, expire_timeout: i) -> (u)`: Creates or updates a notification.
    - Limits: `app_name <= 64` bytes, `summary <= 256` bytes, `body <= 4096` bytes, `actions <= 8` pairs, `hints <= 32`.
    - Token-bucket rate limit: 10 burst, 2/s sustained per unique bus connection.
    - Rate limit error: `org.freedesktop.DBus.Error.LimitsExceeded`.
    - Replacement: `replaces_id` allowed only from same D-Bus sender; unauthorized replaces return error `org.freedesktop.DBus.Error.AccessDenied`.
  - `CloseNotification(id: u) -> ()`: Dismisses an active notification. Only original sender can invoke this; unauthorized callers rejected with `org.freedesktop.DBus.Error.AccessDenied`. Emits `NotificationClosed(id, 3)`.
- Signals:
  - `NotificationClosed(id: u, reason: u)`: Emitted when notification is closed (1=Expired, 2=Dismissed by user, 3=Closed via CloseNotification, 4=Undefined).
  - `ActionInvoked(id: u, action_key: s)`: Emitted via unicast strictly to the original sender who created the notification.
- Test commands:
  ```bash
  busctl --user call org.freedesktop.Notifications /org/freedesktop/Notifications org.freedesktop.Notifications GetServerInformation
  busctl --user call org.freedesktop.Notifications /org/freedesktop/Notifications org.freedesktop.Notifications GetCapabilities
  busctl --user call org.freedesktop.Notifications /org/freedesktop/Notifications org.freedesktop.Notifications Notify susssasa{sv}i "test-app" 0 "" "Summary" "Body" 0 0 5000
  ```

### `org.kynexos.Daemon.Notifications1`
Internal notification presentation and model interface for `kynex-shell`. Bound to `/org/kynexos/Daemon/Notifications` under service name `org.kynexos.Daemon`.
- Object path: `/org/kynexos/Daemon/Notifications`
- Methods:
  - `Dismiss(id: u) -> ()`: Dismisses notification `id` (emits `NotificationClosed(id, 2)` to original sender).
  - `InvokeAction(id: u, key: s) -> ()`: Invokes action `key` on notification `id` (emits `ActionInvoked(id, key)` to original sender).
  - `PauseTimeout(id: u) -> ()`: Pauses expiration countdown for notification `id` (e.g. while hovered in UI).
  - `ResumeTimeout(id: u) -> ()`: Resumes expiration countdown for notification `id` with remaining time.
  - `GetHistory() -> (a(ussssayby))`: Returns array of up to 200 past notifications: `(id, app_name, app_icon, summary, body, actions, app_name_verified, urgency)`.
- Signals:
  - `Shown(id: u, app_name: s, app_name_verified: b, app_icon: s, summary: s, body: s, actions: as, urgency: y, timeout_ms: i)`: Emitted when a notification popup should be displayed on screen.
  - `Updated(id: u, app_name: s, app_name_verified: b, app_icon: s, summary: s, body: s, actions: as, urgency: y, timeout_ms: i)`: Emitted when an active notification popup is updated.
  - `Closed(id: u, reason: u)`: Emitted when a notification popup is removed/closed.
- Properties:
  - `DoNotDisturb` (`b`, read-write): When `true`, suppresses transient popups for low/normal urgency notifications. Critical urgency notifications still pop up.
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Notifications org.kynexos.Daemon.Notifications1
  busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon/Notifications org.kynexos.Daemon.Notifications1 DoNotDisturb
  busctl --user set-property org.kynexos.Daemon /org/kynexos/Daemon/Notifications org.kynexos.Daemon.Notifications1 DoNotDisturb b true
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Notifications org.kynexos.Daemon.Notifications1 GetHistory
  ```

### `org.kde.StatusNotifierWatcher` & `org.freedesktop.StatusNotifierWatcher`
Standard StatusNotifierWatcher service implementing the FreeDesktop / KDE StatusNotifierItem specification. Implemented by `kynex-daemon` and bound to `/StatusNotifierWatcher`. Acquired with non-replacement flags (`DO_NOT_QUEUE`, never replacing existing owners).
- Service names: `org.kde.StatusNotifierWatcher` and `org.freedesktop.StatusNotifierWatcher` (when acquired)
- Object path: `/StatusNotifierWatcher`
- Methods:
  - `RegisterStatusNotifierItem(service: s) -> ()`: Registers an application's StatusNotifierItem.
    - Argument can be an object path starting with `/` (sender is caller's unique bus name) or a service name (`service` or `service/path`).
    - Sender Verification Invariant: Argument MUST resolve to the sender's own connection bus name. Clients cannot register another application's service.
    - Limits: Max 4 items per sender; max 64 items in total across the session. Excess calls rejected with `LimitsExceeded`.
  - `RegisterStatusNotifierHost(service: s) -> ()`: Registers a visualization panel host (such as `kynex-shell`).
- Signals:
  - `StatusNotifierItemRegistered(service: s)`: Emitted when an item is registered.
  - `StatusNotifierItemUnregistered(service: s)`: Emitted when an item is unregistered or its owner disconnects.
  - `StatusNotifierHostRegistered()`: Emitted when a new host registers.
  - `StatusNotifierHostUnregistered()`: Emitted when a host disconnects.
- Properties:
  - `RegisteredStatusNotifierItems` (`as`, read-only): Array of currently registered item identifiers.
  - `IsStatusNotifierHostRegistered` (`b`, read-only): `true` if at least one visualization host is active; `false` otherwise.
  - `ProtocolVersion` (`i`, read-only): Implemented protocol version (`0`).
- Test commands:
  ```bash
  busctl --user introspect org.kde.StatusNotifierWatcher /StatusNotifierWatcher org.kde.StatusNotifierWatcher
  busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems
  busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher org.kde.StatusNotifierWatcher IsStatusNotifierHostRegistered
  busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher org.kde.StatusNotifierWatcher ProtocolVersion
  ```

### `org.kynexos.Daemon.Tray1`
Internal system tray presentation and interaction interface for `kynex-shell`. Bound to `/org/kynexos/Daemon/Tray` under service name `org.kynexos.Daemon`.
- Object path: `/org/kynexos/Daemon/Tray`
- Methods:
  - `GetItems() -> (a(ssssssba(iiay)s))` (or structured `TrayItemInfo`): Returns list of currently active items with sanitized metadata and validated icon pixmaps.
  - `Activate(id: s, x: i, y: i) -> ()`: Asynchronously invokes `Activate(x, y)` on the target item.
  - `SecondaryActivate(id: s, x: i, y: i) -> ()`: Asynchronously invokes `SecondaryActivate(x, y)` on the target item.
  - `ContextMenu(id: s, x: i, y: i) -> ()`: Asynchronously invokes `ContextMenu(x, y)` on the target item with global window coordinates when item does not export a dbusmenu object path.
  - `Scroll(id: s, delta: i, orientation: s) -> ()`: Asynchronously invokes `Scroll(delta, orientation)` on the target item.
  - `GetMenu(item_id: s) -> (revision: u, items: a(iisbbuuiisb))`: Returns sanitized, flat, ordered list of menu items for target item. Flat array entries encode: `(id i, parent i, label s, enabled b, visible b, kind u {0: normal, 1: separator}, toggle u {0: none, 1: checkbox, 2: radio}, toggle_state i {-1: indeterminate/unknown, 0: unchecked, 1: checked}, icon_name s, disposition u {0: normal, 1: informative, 2: warning, 3: alert}, shortcut s, has_submenu b)`.
  - `MenuAboutToShow(item_id: s, menu_item_id: i) -> (need_update: b)`: Informs the item's dbusmenu service that submenu `menu_item_id` is about to be displayed; returns `need_update` flag and triggers bounded re-fetch if updated.
  - `MenuEvent(item_id: s, menu_item_id: i, event: s, revision: u) -> ()`: Dispatches event (`"clicked"` or `"hovered"`) to the item's dbusmenu service for `menu_item_id` if validated against current layout and revision matches current cached layout revision. If revision does not match, rejected with `org.freedesktop.DBus.Error.InvalidArgs`.
- Signals:
  - `ItemAdded(item: TrayItemInfo)`: Emitted when a new tray item is validated and ready for display.
  - `ItemChanged(item: TrayItemInfo)`: Emitted when an existing item's properties update (rate-limited to 10/s).
  - `ItemRemoved(id: s)`: Emitted when an item is unregistered or its owner disconnects.
  - `MenuChanged(item_id: s, revision: u)`: Emitted when an item's menu layout or properties update.
- Menu Limits & Invariants:
  - Nesting depth: `<= 6`. Excess depth rejects the entire layout.
  - Total items: `<= 500`. Excess item count rejects the entire layout.
  - Children per node: `<= 100`. Excess children reject the entire layout.
  - Unique IDs: Non-negative integers (`>= 0`, root is 0). Duplicate IDs, negative IDs other than root, cycles, or self-ancestor nodes reject the entire layout.
  - Label sanitization: ASCII controls and Unicode bidi overrides/isolates stripped, markup stripped, mnemonic underscores parsed (`_File` -> `File`, `__` -> `_`), capped at 256 characters.
  - Type safety: Malformed or mismatched property types ignored with safe defaults.
  - Strict enums: Kind, toggle type, toggle state, and disposition mapped to strict invariants.
  - Theme icons: Icon names validated against Freedesktop icon specification (`[a-zA-Z0-9._-]`, <= 128 chars); raw filesystem paths rejected; raw raster byte data (`icon-data`) ignored.
  - Filter rules: Invisible items (`visible == false`) dropped; disabled items kept with `enabled = false`.
  - Event validation: Allowed only for IDs in CURRENT validated layout, enabled, not separators; events restricted to `"clicked"` and `"hovered"`.
  - Call bounds: 2.0 s timeout on all dbusmenu calls; max 8 concurrent fetches; per-item rate limit of 10 refreshes/second. Menus fetched lazily on demand; cache evicted when item unregisters or owner vanishes.
- dbusmenu Protocol Omissions & Compatibility Notes (AUD-P-07):
  The `com.canonical.dbusmenu` specification defines a broader surface of batch query methods, signals, and properties. In accordance with Kynex OS security invariants and low-overhead architecture (see ADR-015 and Decision D-052), the following elements are intentionally not implemented:
  - `GetGroupProperties(ids: ai, property_names: as)`: Omitted because Kynex fetches the entire validated layout in a single bounded `GetLayout(0, -1, [])` roundtrip. Partial batch queries create state synchronization ambiguity and multi-roundtrip latency. Applications falling back to `LayoutUpdated` refreshes behave identically.
  - `AboutToShowGroup(ids: ai)`: Omitted because Kynex manages menus as discrete hierarchy views, notifying only the active submenu via `MenuAboutToShow(item_id, submenu_id)`. Multi-group pre-flight notification adds unnecessary roundtrip IPC without practical benefit for tray menus.
  - `EventGroup(events: a(isvu))` / `ItemActivationRequested(id: i, timestamp: u)`: Omitted because user interactions are dispatched strictly per discrete user gesture. Batching interactive events or delegating activation requests to external clients introduces race conditions and out-of-order execution risks.
  - `Version` property: Omitted as redundant with D-Bus introspection and fixed interface schema.
  - `Status` property: Omitted because application status is already tracked authoritatively via `StatusNotifierItem.Status` (`Active`, `Passive`, `NeedsAttention`).
  - `IconThemePath` property: Omitted to enforce process boundary and security isolation (`.agents/rules/02-security.md`). Kynex strictly rejects scanning arbitrary filesystem paths provided by untrusted client applications, preventing path traversal and local asset injection. Icons must resolve via standard system icon themes or fall back to deterministic letter badges.
- Test commands:
  ```bash
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 GetItems
  busctl --user call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 GetMenu s ":1.42/StatusNotifierItem"
  ```

## Template for each interface
- Object path:
- Methods:
- Signals:
- Properties:
- Test command (`busctl --user ...`):
