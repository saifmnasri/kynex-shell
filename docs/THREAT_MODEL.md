# Threat model (living document)

## Assets
User files and credentials, session integrity (lock screen), clipboard and notification contents, window titles, network/Bluetooth state, the update channel.

## Trust boundaries
1. Other apps (untrusted) -> our D-Bus services (Notifications, StatusNotifier, portals).
2. KWin scripting environment -> private feeder interface `org.kynexos.Daemon.WindowsFeed1`.
3. Shell UI and user presentation -> untrusted window metadata (titles, application classes).
4. User session -> system services (polkit, logind, udisks2, NetworkManager).
5. Our packages/update channel -> user's machine.
6. Files and thumbnails from disk (untrusted content) -> our parsers.

## Main threats and mitigations
| Threat | Mitigation |
|---|---|
| Malicious app spams/forges notifications, injects markup | Treat all notification text as untrusted: strip/escape markup; rate-limit (token-bucket burst 10, 2/s refill); cap sizes (app_name 64B, summary 256B, body 4KiB, actions <= 8, hints <= 32); strip controls and bidi overrides (U+202A-202E, U+2066-2069); validate raw image-data <= 256x256, 8-bit, 3-4 ch, checked arithmetic; enforce caller authorization on replaces_id and CloseNotification; unicast ActionInvoked; layer-shell KeyboardInteractivityNone (no focus stealing); dynamic input mask click pass-through |
| Malicious tray item / icon data | Validate sizes and formats; decode in a limited-size path; no file paths from clients |
| Spoofed window feed (`WindowsFeed1`) | Layered, fail-closed feeder authentication: 1) verify caller UID matches daemon session UID; 2) verify caller unique bus name matches current `GetNameOwner("org.kde.KWin")` on every call (tracked via `NameOwnerChanged`); 3) verify caller `/proc/<pid>/exe` equals EXACTLY `/usr/bin/kwin_wayland` (no filename or suffix fallbacks). If `/proc/<pid>/exe` is unreadable due to kernel non-dumpable file capabilities (`cap_sys_nice=ep`), rely on 1+2 and log once. Reject all other inspection errors. Residual risk: a same-UID rogue process starting before KWin could claim `org.kde.KWin` causing DoS to KWin, though spoofing feed data still requires elevated/root capabilities to simulate capability-restricted `/proc` state. |
| Window title spoofing / bidi override / injection | Treat all window titles as untrusted: enforce max 256 bytes sliced on UTF-8 char boundary, strip ASCII control characters (0x00-0x1F, 0x7F) and Unicode directional/bidi overrides (U+202A-202E, U+2066-2069), validate appId against `[A-Za-z0-9._-]` max 128 chars |
| Window storm denial-of-service | Cap total tracked windows at 512; enforce per-sender rate limit of 200 calls/second with dropped-call counter; drop excess updates without panicking or allocating unboundedly |
| D-Bus method abuse | Session bus only; validate every argument; check caller (peer credentials) for sensitive calls; no shell execution with client strings |
| Launching arbitrary commands via .desktop `Exec` | Freedesktop Desktop Entry Specification v1.5 compliant parser with hard limits (file <= 64 KiB, line <= 4 KiB, groups <= 50, keys <= 200); safe field code expansion (`%f`, `%F`, `%u`, `%U`, `%i`, `%c`, `%k`, `%%`, deprecated dropped); strict tokenization into `Vec<String>` argv without ever invoking a shell (`sh -c` is never used); `Launch(id)` executes binary directly via `tokio::process::Command` with `process_group(0)` detached session, null stdin/stdout/stderr to prevent fd leaks, strict environment allowlist (`HOME`, `PATH`, `XDG_*`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `LANG`/`LC_*`, `DISPLAY`), caller UID verification, and 5 launches/sec daemon-wide rate limit. Spawned children are reaped by bounded asynchronous waiter tasks without zombies; surviving children reparented to user manager subreaper on daemon restart (ADR-008). |
| Spoofed shell shortcut feed (`ShellFeed1`) | Layered, fail-closed feeder authentication matching `WindowsFeed1` (session UID, KWin bus owner, `/proc/<pid>/exe` == `/usr/bin/kwin_wayland`). Residual threat bounded: worst a spoofed toggle can do is open/close the launcher overlay. |
| Untrusted .desktop display text / bidi spoofing / HTML injection | Treat all desktop metadata as untrusted: sanitize names/comments in Rust by stripping ASCII controls (0x00-0x1F, 0x7F) and Unicode bidi overrides/isolates (U+202A-202E, U+2066-2069), cap lengths (128 / 256 chars), and render exclusively with `textFormat: Text.PlainText` across all QML surfaces. |
| Fake lock screen (overlay instead of real lock) | Use the session-lock mechanism KWin/logind provide; never fake locking with a normal window; verify before Phase 8 ships |
| Path traversal / symlink tricks in file manager | Canonicalize, use `openat`-style APIs where possible, never follow symlinks across trust boundaries on delete/move |
| Malicious thumbnails/media | Generate thumbnails in a sandboxed child process with resource limits; fuzz parsers |
| Supply chain (crates, CI actions, packages) | `cargo-deny`, `cargo-audit` nightly, Dependabot, pinned actions, `Cargo.lock`, SBOM, signed tags/commits, reviewed new deps |
| Secrets leaking into git/logs | Secret scanning (local + CI + GitHub push protection), `.gitignore`, logging rules |
| Privilege escalation via our polkit agent | Agent only shows prompts; never handles passwords outside the polkit protocol; no password logging |
| Update tampering | Updates via signed APT repository only; no custom updater downloading binaries |
| Malicious or buggy style pack (QML is executable) | Load only from system/user styles dirs, validate manifest, restrict imports, no raw D-Bus access, warn on unsigned packs, signing before any remote install |
| Memory safety | Rust by default; `unsafe` only in FFI, reviewed, `// SAFETY:` documented; fuzz and test boundaries |

## Out of scope for now
Full app sandboxing (Flatpak/portals later), disk encryption, secure boot chain (distro phase).

## Systemd User Unit Sandboxing on Ubuntu 26.04

Kynex background daemons execute as unprivileged systemd user units (`systemd --user`). Every sandboxing directive was empirically probed via `scripts/test-systemd-unit.sh` using isolated `systemd-run --user` commands. Directives are strictly classified into effective vs not available on this platform.

### 1. Empirically Effective Directives (Packaged in `kynex-daemon.service`)
The packaged unit includes exclusively directives verified effective without root or elevated capabilities:
- `NoNewPrivileges=yes`: Enforces `prctl(PR_SET_NO_NEW_PRIVS)` (`NoNewPrivs: 1` in `/proc/<pid>/status`). Prevents execution of SUID/SGID binaries and capability gains.
- `RestrictAddressFamilies=AF_UNIX`: Installs seccomp socket filter blocking `AF_INET`/`AF_INET6`/`AF_PACKET` with `EAFNOSUPPORT`. Only local UNIX domain sockets are permitted.
- `SystemCallFilter=@system-service`: Installs seccomp filter (`Seccomp: 2` in `/proc/<pid>/status`) whitelisting standard system service syscalls and killing non-whitelisted calls (e.g. `reboot`) with `SIGSYS`.
- `SystemCallArchitectures=native`: Enforces seccomp filter to reject foreign architecture syscall emulation.
- `RestrictRealtime=yes`: Seccomp filter blocks real-time scheduling calls (`sched_setscheduler`, `sched_setparam`) with `EPERM`.
- `RestrictNamespaces=yes`: Seccomp filter blocks creation of new user, mount, network, or UTS namespaces (`CLONE_NEW*` / `unshare(2)`) with `EPERM`.
- `RestrictSUIDSGID=yes`: Seccomp filter denies creation or chmod of set-user-ID / set-group-ID bits (`chmod(04755)`) with `EPERM`.
- `MemoryDenyWriteExecute=yes`: Seccomp filter prevents allocation of memory mappings that are simultaneously writable and executable (`PROT_WRITE | PROT_EXEC`) with `EACCES`.
- `LockPersonality=yes`: Seccomp filter locks execution domain; changing ABI personality returns `EPERM`.
- `KeyringMode=private`: Kernel allocates an isolated session keyring distinct from the default user session keyring.
- `UMask=0077`: Enforces strict owner-only file creation mask (mode 0600 files, mode 0700 directories).

### 2. Ineffective Directives ("Not available for user units on this platform")
The following directives are omitted from the packaged unit because unprivileged user managers on Ubuntu cannot isolate namespaces or load cgroup BPF programs without root (`CAP_SYS_ADMIN`, `CAP_NET_ADMIN`, `CAP_BPF`):
- `ProtectSystem=strict`, `ProtectHome=read-only`, `PrivateTmp=yes`, `ProtectKernelTunables=yes`, `ProtectControlGroups=yes`: Require unsharing the mount namespace (`CLONE_NEWNS`). On Ubuntu, unprivileged user namespaces are restricted (`kernel.apparmor_restrict_unprivileged_userns = 1`), and the user manager does not create mount namespaces for user services. Filesystem paths remain in the host user mount namespace (`/proc/<pid>/ns/mnt` matches session).
- `PrivateNetwork=yes`: Requires unsharing the network namespace. Fails with systemd journal warning: *"PrivateNetwork=yes is configured, but the kernel does not support or we lack privileges for network namespace, proceeding without."* Host network interfaces remain accessible.
- `ProtectHostname=yes`: Requires unsharing the UTS namespace. Fails with systemd journal warning: *"ProtectHostname=yes is configured, but UTS namespace setup is prohibited (container manager?), ignoring namespace setup."*
- `IPAddressDeny=any`: Requires attaching BPF programs to cgroups via `BPF_PROG_ATTACH` (`CAP_BPF` / `CAP_NET_ADMIN`). Silently ignored by unprivileged user manager; outbound network connections succeed.
- `ProtectProc=invisible`, `ProcSubset=pid`: Require remounting `/proc` with `hidepid=invisible` or `subset=pid` inside a mount namespace. Ineffective because mount namespace is shared; host processes and `/proc/cmdline` remain visible.

### 3. Desktop Entry Files and Exec Lines as Untrusted Input (Phase 4 Slice 1)

Application `.desktop` files discovered in standard XDG paths (`$XDG_DATA_HOME/applications` and `$XDG_DATA_DIRS/applications`) and user directories are treated as untrusted external input. An attacker with local user filesystem access could craft adversarial `.desktop` entries to achieve code execution or denial of service against `kynex-daemon`.

#### Threat Vectors & Parser Mitigations
1. **Shell Command Injection in `Exec` Lines**:
   - *Attack*: Embedding shell metacharacters (`; rm -rf ~`, `$(reboot)`, backticks, pipelines `|`, redirects `>`, or unclosed quotes) inside `Exec=` fields.
   - *Mitigation*: The `parse_exec` parser tokenizes the command line according to Desktop Entry Specification Section "The Exec key" into a discrete `Vec<String>` array of arguments. It **NEVER invokes a shell** (`sh`, `bash`, or `system(3)` are never executed). Arguments are passed directly to `std::process::Command::new(&argv[0]).args(&argv[1..])`. Shell syntax characters are treated purely as literal string arguments.
2. **Denial-of-Service via Pathological File Inputs**:
   - *Attack*: Endless lines (> 100 KB), massive files (> 10 MB), infinite directory nesting, or cyclic symlinks designed to trigger OOM, stack overflow, or infinite loops.
   - *Mitigation*: Hard limits enforced: file size <= 64 KiB (`MAX_FILE_SIZE`), line length <= 4 KiB (`MAX_LINE_LENGTH`), group count <= 50 (`MAX_GROUPS`), keys per group <= 200 (`MAX_KEYS_PER_GROUP`). Directory traversal uses canonical path tracking in a `HashSet` to detect and break symlink cycles.
3. **Blocking I/O & FIFO / Device Node Traversal**:
   - *Attack*: Creating named pipes (FIFOs) or special device files (e.g. `/dev/urandom` or blocked FIFOs) named `malicious.desktop` to hang daemon directory scanning.
   - *Mitigation*: Scanner checks `entry.file_type()` and verifies `fs::metadata(path)?.is_file()` before opening, explicitly rejecting FIFOs, sockets, and character/block devices (`DesktopEntryError::NotARegularFile`).
4. **Environment & Privilege Leakage on Launch**:
   - *Attack*: Injecting adversarial environment variables or inheriting daemon open file descriptors.
   - *Mitigation*: On launch, `env_clear()` wipes all inherited environment variables, repopulating only a strict allowlist (`HOME`, `PATH`, `XDG_*`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `LANG`, `LC_*`, `DISPLAY`). Stdin, stdout, and stderr are bound to `/dev/null`.
5. **Zombie Process Accumulation**:
   - *Attack*: Launching hundreds of transient processes that terminate immediately, accumulating defunct zombie entries if the daemon does not wait for them.
   - *Mitigation*: `spawn_detached` sets `process_group(0)` and delegates each child to an asynchronous Tokio waiter task tracked by an atomic gauge, which reaps child exit status automatically and prevents defunct zombies while the daemon is running. Launch rate is bounded daemon-wide to 5 launches/sec. Upon daemon restart or termination, surviving processes are reparented to the session subreaper (systemd user manager / PID 1), which reaps exit status automatically (ADR-008).
6. **Caller Authorization & Launch Rate Limiting**:
   - *Attack*: Compromised local client flooding `Launch(id)` across multiple connections to cause system fork-bombing.
   - *Mitigation*: `Launch` verifies the D-Bus caller UID matches the daemon session UID via `GetConnectionCredentials`. A daemon-wide sliding-window rate limit caps launches to 5 per second across all connections. `Launch` logs only the sanitized desktop file ID, never arbitrary command arguments.

### 4. Launcher Overlay, Display Text Sanitization, and Shortcut Feeding (Phase 4 Slice 2)

The launcher overlay (`Launcher.qml` and `LauncherModel`) presents indexed application metadata to the user, responds to compositor global shortcuts, and issues asynchronous search queries to `org.kynexos.Daemon.Apps1`.

#### Threat Vectors & Mitigations
1. **Untrusted Metadata Display & HTML / Bidi Spoofing**:
   - *Attack*: Malicious `.desktop` files embedding HTML tags (`<img src=...>`, `<script>`, `<font color=red>`), ANSI terminal control characters, or Unicode bidirectional override/isolate sequences (U+202A-202E, U+2066-2069) in `Name`, `GenericName`, or `Comment` fields to invert display text, spoof system dialogs, or exploit rich text rendering engines.
   - *Mitigation*: Metadata is sanitized in pure Rust (`launcher_logic::sanitize_untrusted_text`): strips all ASCII controls (0x00-0x1F, 0x7F) and Unicode bidi override characters, normalizes whitespace, and clamps lengths (names <= 128 chars, comments <= 256 chars). In QML, all text components (`KynexListItem.qml`, `KynexSearchField.qml`, `Launcher.qml`) explicitly specify `textFormat: Text.PlainText`, preventing interpretation of HTML or XML entities.
2. **Shortcut Feeder Spoofing (`ShellFeed1.LauncherShortcut`)**:
   - *Attack*: Unprivileged local processes calling `LauncherShortcut()` to spam or toggle the launcher overlay window.
   - *Mitigation*: The feeder interface enforces the exact same layered, fail-closed `SenderPolicy` as `WindowsFeed1`: verifies caller session UID, unique bus name owner of `org.kde.KWin`, and executable path `/usr/bin/kwin_wayland`.
   - *Residual Impact*: Even if authorization were compromised, the method is strictly a toggle without parameters; the worst impact is opening or closing the launcher overlay window on the desktop. It cannot execute arbitrary code or escalate privileges.
3. **Asynchronous Query Reordering / Out-of-Order Search Races**:
   - *Attack*: Rapid keystrokes dispatching asynchronous D-Bus search calls that arrive out of order, causing an older, slow query response to overwrite the results of a newer query.
   - *Mitigation*: Search tracking enforces a strictly monotonically increasing sequence number in pure Rust (`SearchTracker`). Replies carrying older sequence numbers are discarded immediately, ensuring latest-wins behavior at all times.
4. **Query Injection and Resource Exhaustion**:
   - *Attack*: Sending queries containing thousands of characters or unbounded result requests to exhaust memory.
   - *Mitigation*: Query strings are trimmed and hard-capped at 128 characters; result limits are clamped to at most 50 items.

### 5. Icon Files, Image Decoders, and Pinned Applications as Untrusted Input (Stage M)

Icon theme directories (`$HOME/.icons`, `$XDG_DATA_HOME/icons`, `/usr/share/icons`), image files on disk, and `$XDG_CONFIG_HOME/kynex/pinned.toml` are treated as untrusted external input.

#### Threat Vectors & Mitigations
1. **Path Traversal & Symlink Escape via Icon Names**:
   - *Attack*: Providing adversarial icon names with traversal sequences (`../../../../etc/shadow`), path separators (`/`, `\`), or NUL bytes, or crafting symlink chains pointing outside icon roots.
   - *Mitigation*: Icon names validated against strict charset `^[a-zA-Z0-9._-]+$` with length <= 128 characters. Path separators and traversals rejected immediately. Symlink targets are canonicalized and verified to reside strictly within allowed roots (`IconRoots`). Absolute path icons from `.desktop` files are accepted only if they resolve inside allowed icon roots or standard XDG data directories.
2. **Denial-of-Service via Hostile `index.theme` Files**:
   - *Attack*: Pathological `index.theme` files (> 100 MB, endless lines, or cyclic theme inheritance loops like `A -> B -> A`).
   - *Mitigation*: Hard limits enforced: file size <= 64 KiB (`MAX_THEME_FILE_SIZE`), line length <= 512 bytes (`MAX_THEME_LINE_LENGTH`), total lines <= 2,048 (`MAX_THEME_LINES`). Theme inheritance (`Inherits=`) tracks visited theme names with cycle detection and a hard inheritance depth cap of 8.
3. **Image Decompression Bombs & Memory Exhaustion**:
   - *Attack*: Raster decompression bomb headers (e.g. 60,000 x 60,000 pixels compressed into a tiny PNG), or massive image allocations.
   - *Mitigation*: Global `QImageReader::setAllocationLimit(16)` (16 MB) configured at shell initialization. Raster header dimensions inspected prior to decoding; headers exceeding 512x512 pixels rejected immediately without decompression. Max file size capped at 1 MiB (`MAX_ICON_FILE_SIZE`). Non-regular files (FIFOs, sockets, device nodes) rejected before reading.
4. **Hostile SVG XML Attacks (XXE, External Resource Exfiltration, Use-Bombs)**:
   - *Attack*: SVG containing external DTDs (`<!DOCTYPE`), external entity declarations (`<!ENTITY`), CSS `@import`, embedded scripts (`<script>`, `javascript:`), external image references (`<image href="file:///etc/passwd">`), or exponential expansion bombs (`<use>` nesting).
   - *Mitigation*: Pure-Rust/C++ security filter (`kynex_is_svg_safe`): byte-level rejection of `<!DOCTYPE`, `<!ENTITY`, `@import`, `file://`, and `ftp://`; strict XML stream tokenization rejecting `<script>` tags, inline `javascript:` URIs, `<use>` expansion count > 64, external `<use>` references (must start with `#`), and external `<image>` / `<feImage>` references (must be embedded `data:image/` URIs). On any decoding error, UI falls back cleanly to the deterministic initial letter badge without panicking.
5. **Asynchronous Presentation Threading & Texture Lifecycle**:
   - *Attack*: Blocking the GUI/render thread during image lookup or I/O, or double-free / use-after-free in async image response handling.
   - *Mitigation*: Async icon resolution via `QQuickAsyncImageProvider` executed in `QThreadPool`. Thread-safe `QImage` value storage in `KynexIconResponse` with on-demand `QQuickTextureFactory::textureFactoryForImage` returning caller-owned texture factories, preventing cross-thread lifetime races.
6. **Pinned Applications Persistence Tampering & Corruption**:
   - *Attack*: Concurrent mutation floods, corrupt `pinned.toml`, disk full / power failure during write, symlink swaps, or unauthorized process mutation of user pins.
   - *Mitigation*: `org.kynexos.Daemon.Pins1` enforces caller UID validation (`check_same_uid`). Global sliding-window rate limit of 10 mutations/second. Atomic persistence: writes to temporary file `.pinned.toml.tmp.XXXXXX` in same directory with mode `0600`, calls `fsync`, and executes atomic rename. File size bounded <= 64 KiB, pins capped at 128 items. Corrupt files are backed up to `pinned.toml.bad.<timestamp>` and safely initialized to an empty list without panicking. Unknown desktop IDs are preserved across saves but flagged as missing.


### 6. Desktop Notifications as Untrusted Input (Stage N)

All data arriving via `org.freedesktop.Notifications.Notify` is treated as untrusted external input from arbitrary local user processes.

#### Threat Vectors & Mitigations
1. **Markup, Script, and Resource Injection**:
   - *Attack*: Adversary sends notifications containing `<script>alert('XSS')</script>`, `<img src="file:///etc/shadow">`, `<a href="javascript:...">`, or styling tags to execute scripts, leak local files, or exploit rich text layout engines.
   - *Mitigation*: Advertised capabilities strictly exclude `body-markup`. All input text passes through pure-Rust sanitization (`kynex_notifications::sanitize_text`): strips all XML/HTML markup tags, completely drops `<script>` and `<style>` blocks along with their inner contents, decodes safe standard entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, numeric entities), and truncates strictly on valid UTF-8 character boundaries. In QML, all popup text elements (`NotificationPopup.qml`) enforce `textFormat: Text.PlainText`.
2. **Unicode Bidi Overrides and Control Character Spoofing**:
   - *Attack*: Using Unicode bidirectional override/isolate characters (U+202A..=U+202E, U+2066..=U+2069) or terminal escape sequences to invert text layout, spoof system security prompts, or disguise malicious sender domains.
   - *Mitigation*: The sanitizer strips all ASCII C0 control characters (except `\n` and `\t`), ASCII DEL (`0x7F`), and Unicode bidi override/isolate characters before string construction.
3. **Pathological Image-Data Payloads**:
   - *Attack*: Supplying adversarial `image-data` hints with deceptive dimensions (e.g. 65,535 x 65,535), negative rowstrides, mismatched buffer lengths, or integer overflow products designed to trigger buffer overreads/overwrites or out-of-memory panics.
   - *Mitigation*: Pure-Rust image validator (`validate_image_data`) enforces: `width` and `height` between 1 and 256; `bits_per_sample == 8`; `channels` exactly 3 (RGB) or 4 (RGBA); `has_alpha` consistent with channel count; checked multiplication for minimum rowstride (`width * channels`); rowstride non-negative; exact buffer length validation (`data.len() == rowstride * height`); and total payload capped at 256 KiB (`MAX_IMAGE_DATA_BYTES`). Any violation rejects the hint cleanly with `NotificationError::InvalidImageData`.
4. **Sender Identity Spoofing & Unverified App Names**:
   - *Attack*: Unprivileged client claims `app_name = "System Security"` or `"Kynex Shell"` to deceive the user into clicking malicious actions or revealing secrets.
   - *Mitigation*: `app_name` is treated as untrusted client text. The daemon cross-references the `desktop-entry` hint against the verified `Apps` index. If resolved to an authentic installed desktop file, the verified desktop name and canonical icon are used and `appNameVerified` is flagged `true`. If unresolvable or missing, the sanitized client `app_name` is displayed alongside an unverified indicator badge (`appNameVerified = false`).
5. **Notification Hijacking & Unauthorized Modification**:
   - *Attack*: Client guesses another application's notification ID and attempts to replace it via `replaces_id` or dismiss it via `CloseNotification` to suppress security alerts.
   - *Mitigation*: The daemon records the caller's unique D-Bus connection name upon notification creation. Calls to `Notify` with `replaces_id > 0` or calls to `CloseNotification` from any sender other than the original creator are rejected with `NotificationError::SenderMismatch`.
6. **Action Eavesdropping & Unicast Delivery**:
   - *Attack*: Eavesdropping local D-Bus clients monitoring broadcast signals to intercept interactive action callbacks (e.g. multi-factor authentication approvals or private reply content).
   - *Mitigation*: `ActionInvoked` signals are dispatched as unicast D-Bus messages strictly targeted to the original sender's unique bus name. If the original sender disconnects before action invocation, no signal is broadcast.
7. **Resource Exhaustion & Flooding Attacks**:
   - *Attack*: Flooding millions of notifications or massive string payloads to exhaust memory or CPU.
   - *Mitigation*: Text fields are hard-capped: `app_name <= 64` B, `summary <= 256` B, `body <= 4096` B, `actions <= 8` pairs (key <= 32 B, label <= 64 B), `hints <= 32`. Per-sender token bucket rate limiting permits bursts of up to 10 notifications with a sustained refill rate of 2 per second; exceeding senders receive `NotificationError::RateLimitExceeded`. Active sender tracking is bounded to 256 entries with 60-second idle pruning. Visible popups are capped at 5 with excess queued in a FIFO buffer; notification history is bounded to a ring buffer of 200 entries.
8. **Focus Stealing & Desktop Input Interception**:
   - *Attack*: Notification popup surface stealing keyboard focus or blocking mouse clicks destined for underlying desktop windows.
   - *Mitigation*: The notification overlay uses `LayerSurface` with `KeyboardInteractivityNone`, preventing it from ever receiving keyboard focus. Furthermore, dynamic input region masks (`LayerSurface.setInputMask` / `QWindow::setMask`) limit the Wayland surface input region strictly to the bounding boxes of visible popup cards, ensuring all clicks and pointer events outside active popups pass through transparently to underlying windows.
