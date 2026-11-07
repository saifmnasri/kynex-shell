# Roadmap

Rule: do the first unchecked step. Each phase has **exit criteria**; do not start the next phase until they pass (`./scripts/audit.sh` + the listed checks). Agents: tick boxes only with evidence. Scope order: finish the desktop environment first; the distro (ISO/installer/own repo) comes after v1.0.

## Phase 0 — Environment, repo and baselines
- [x] Setup done via SETUP.md prompt: `check-env.sh` all PASS, GitHub repo created
- [x] Cargo workspace from `templates/Cargo.workspace.toml` with crates `kynex-common`, `kynex-ipc`, `kynex-style`, `kynex-daemon`, `kynex-shell`; CI workflow verified green on PR #5 (run 41951258172)
- [ ] Baselines recorded in `docs/benchmarks/`: stock Ubuntu session (already in the VM), KDE Plasma (clone VM with Kubuntu) via `/baseline-plasma`
**Exit:** CI green on `main`, hooks active, baseline numbers committed.

## Phase 1 — Skeleton + style engine
- [x] `kynex-style`: manifest/tokens parser + validator (+ malicious-manifest tests)
- [x] `LayerSurface` core type (layer-shell shim) + `Clock` object; `styles/default` draws a bottom panel with a live clock using `Kynex.Style` tokens
- [x] `dev/run-nested.sh` (dbus-run-session, isolated user directories in dev/tmp); `kynex.desktop` + `startkynex` written (not installed)
- [x] Headless smoke test with `kwin_wayland --virtual`: shell starts, no QML warnings
**Exit:** one command runs everything; smoke test in CI.

## Phase 2 — Daemon, IPC, tokens and components
- [x] `kynex-ipc` + `kynex-daemon` owning `org.kynexos.Daemon`; hardened systemd user unit (Verified in live user manager session with scripts/test-systemd-unit.sh; all 21 directives probed and documented in THREAT_MODEL.md, unit runs with zero journal warnings)
- [x] D-Bus integration test harness (private bus, fake services) (Verified with private dbus-daemon, FakeUPowerDevice and FakeNetworkManager in tests/; deterministic across repeated test runs)
- [x] `Kynex.Style` tokens API + base components (KynexButton, KynexSwitch, KynexSlider, KynexTooltip, KynexListItem) + visual regression tests (Implemented in `Kynex.Controls` with zero numeric literals except 0/1; 20 Qt Quick Test unit tests in `scripts/test-qml-controls.sh` passing deterministically; visual regression test suite in `scripts/visual-test.sh` across dark/light, 100%/200% scale, and RTL verified 10/10 runs)
- [x] Status indicators: battery (UPower) and network (NetworkManager) (Implemented with signal-driven D-Bus clients in `kynex-daemon`, Rust sanitization in `kynex-shell`, lazy-loaded `KynexTooltip` on hover, zero idle PSS regression; Volume and Bluetooth moved to Phase 5 per D-033)
**Exit:** coverage >= 80% on daemon; idle CPU/memory measured vs budget.

## Phase 3 — Windows, taskbar/dock, switcher
- [x] Phase 3 Slice 1: KWin script + `WindowsFeed1` / `Windows1` read-only interfaces + `WindowListModel` + read-only taskbar in `styles/default` (Verified with private D-Bus integration tests 10/10 PASS, live nested KWin session with `dev/test-windows`, 512-window storm zero memory leak, and 85%+ coverage). Window control actions, pinned apps, and launcher deferred to subsequent slices per scope order.
- [x] Phase 3 Slice 2: Window control actions via KWin script (activate, close, minimize, restore) (Verified with private D-Bus integration tests in `windows_service_test.rs`, live nested KWin demo in `dev/demo-window-actions.sh` testing Minimize, Restore, Activate, Close with zero filesystem leakage, and `services/windows.rs` line coverage 93.03%)
- [x] Phase 3 Slice 3 (Stage M): Pinned apps persistence and app grouping (`org.kynexos.Daemon.Pins1`, atomic `pinned.toml`, pure-Rust window-to-app matching in `matching.rs` with 97.67% line coverage, TaskbarModel with pinned apps in pin order and running window grouping count badge, launcher Ctrl+P pin toggling, private D-Bus integration tests, 10x flake verification). Window previews, drag and drop, and alt-tab switcher deferred per scope order.
- [ ] Core models: window previews (deferred), alt-tab switcher, virtual desktop indicator
- [ ] Surfaces in `default`: alt-tab switcher; virtual desktop indicator
**Exit:** update latency < 50 ms; fuzz the window-data parser.

## Phase 4 — Launcher and global search
- [x] Phase 4 Slice 1: `kynex-apps-index` (spec-compliant .desktop parser, safe Exec argv lexing without shell, ranked search, inotify watcher, daemon `Apps1` service with detached launch, 10-minute fuzzing, 5,000-entry benchmark) (Verified with 35 unit/proptests at 94.27% coverage, 2 cargo-fuzz targets with AddressSanitizer, 5,000 entries cold scan 29.2 ms, search p99 3.06 ms < 16 ms budget, live nested KWin launch verification, and Stage K2 audit remediations AUD-K-01 through AUD-K-06 verified with 0 zombie leak, 0.00 cswch/s idle, daemon-wide rate limiting, and 98.9% mutation kill ratio). Launcher surface UI deferred to slice 2 per scope order.
- [x] Phase 4 Slice 2: Launcher overlay on Apps1 (`LauncherModel` CXX-Qt model, pure-Rust logic in `kynex-shell-core` with latest-wins sequence numbers at 97.86% coverage, `KynexSearchField` accessible control, `Launcher.qml` LayerSurface overlay, untrusted metadata PlainText rendering, Meta+Space KWin global shortcut script with layered sender authentication, live nested launch demo, Stage L2 audit remediations AUD-L-01 through AUD-L-05 verified: 100% non-equivalent mutants killed with 223/226 dead and 3 documented equivalents, zero qmllint compiler warnings, honest monotonic latency measured inside shell: warm open p50 3.5 ms / p95 4.3 ms < 16 ms budget; on-demand cold open p50 67.3 ms / p95 74.4 ms in VM llvmpipe; idle pre-warmed cold open p50 51.9 ms / p95 57.8 ms at +0.73 MB PSS cost. Note: nested KWin runs with `--no-global-shortcuts`, so physical keypress shortcut firing remains untested until the real bare-metal session in Phase 8).
- [x] Phase 4 Slice 3 (Stage M): Freedesktop Icon Theme Specification v0.13 compliance (`kynex-icons` library crate with 85.57% line coverage, async QQuickImageProvider in C++ shim, 16 MB allocation clamp, raster header check, SVG external resource filter with hostile tests and 10-minute cargo-fuzz campaign, UI integration with token sizing and fallback letter badges, zero UI stalls).
- [ ] Search providers: apps, files (indexed), settings, calculator/units, commands; ranking
- [ ] Launcher surface (grid + search), keyboard-first; fuzz `.desktop` parser; benchmark 5,000 entries
**Exit:** launcher latency budgets met.

## Phase 5 — Notifications, tray, OSD, quick settings
- [x] Phase 5 Slice 1 (Stage N): `org.freedesktop.Notifications` server (FDO spec v1.2, non-replacement `DO_NOT_QUEUE` bus acquisition, strict markup stripping, bidi/control filtering, checked `image-data` validation <= 256x256 / 8-bit / 3-4 ch / <= 256 KiB, per-sender token-bucket rate limiter, single deadline scheduler with 0 idle wakeups, 0 unwrap/expect/unsafe in `kynex-notifications`), `NotificationModel` CXX-Qt model, `styles/default/NotificationPopup.qml` LayerSurface with zero keyboard interactivity, Wayland input mask pass-through, actions, in-place replacement, close reasons 1-4, DND with critical urgency passthrough, 10-minute cargo-fuzz campaigns (markup + image) 0 crashes, 10x flake verification 0 flakes, line coverage >= 85% per file across all new Rust crates, honest monotonic benchmarks: warm latency p50 7.27 ms / p95 28.80 ms < 50 ms budget, cold latency p50 40.94 ms / p95 61.58 ms in VM llvmpipe, zero idle wakeups (0.00 cswch/s on daemon & shell). History panel UI, tray, and quick settings deferred to subsequent slices per scope order.
- [ ] StatusNotifierWatcher + tray menus
- [ ] OSD and quick-settings surface (Wi-Fi, Bluetooth, audio, brightness, power profile, night light via KWin)
- [ ] Volume (PipeWire/WirePlumber) and Bluetooth (BlueZ) subsystem integration (moved from Phase 2 per D-033)
**Exit:** malicious-notification tests (markup, huge payloads, floods) pass.

## Phase 6 — Desktop surface, clipboard, screenshots
- [ ] Wallpaper manager (background layer), desktop icons/widgets
- [ ] Clipboard history with privacy exclusions
- [ ] Screenshot/recording UI via KWin/portal

## Phase 7 — Kynex Files
- [ ] `kynex-files-core` (UI-free): inotify model, sorting, trash, copy/move with progress, mounts (udisks2), thumbnails in a sandboxed process, archives
- [ ] UI: list/grid/columns, tabs, search, open-with, drag and drop, previews
- [ ] Path-traversal/symlink tests; fuzz file-name handling
**Exit:** opens a 100,000-entry directory without UI stalls.

## Phase 8 — Settings, session, lock, security
- [ ] Landlock self-sandbox for the daemon (enforce LSM filesystem and access restrictions without unprivileged namespace degradation)
- [ ] Kynex Settings (display, sound, network, Bluetooth, power, keyboard, pointing devices, appearance/styles, default apps, language, users, date/time, privacy)
- [ ] Real session login (VM snapshot first), autostart, logout/shutdown menu, idle/power policy
- [ ] Lock screen via the proper session-lock mechanism (research first; never an overlay); login greeter
- [ ] Polkit agent, keyring integration, portals backend decision (reuse vs own)
- [ ] Accessibility pass (keyboard, AT-SPI, high contrast, reduce motion), i18n (en/fr/ar RTL)
**Exit:** manual session checklist passes; security review of this phase.

## Phase 9 — Style packs and visual identity
- [ ] Style switcher (`Style1`, Settings UI), hot reload, safe fallback
- [ ] Three original archetype packs: `dock`, `taskbar`, `overview` (+ at least two look packs)
- [ ] Own icon theme, cursor theme, KWin decoration theme, sounds, wallpapers (licenses recorded)
- [ ] Motion polish at 120 Hz; dark/light/high-contrast; fractional scaling checks
**Exit:** visual tests green for every pack; side-by-side screenshots vs the big desktops for the README.

## Phase 10 — Bundled apps and polish
- [ ] Defaults: terminal, text editor, image viewer, PDF viewer, media player, archive tool (reuse permissive/free first)
- [ ] System monitor, software/update center, diagnostics/log viewer
- [ ] Extension/widget API and `kynexctl` CLI; developer docs

## Phase 11 — Packaging for Ubuntu 26.04 (DE only)
- [ ] `.deb` packages and `kynex-desktop` meta-package (session entry, services, default settings) installable on stock Ubuntu 26.04
- [ ] CI builds packages, SBOM, signed tags; reproducible builds
**Exit:** fresh Ubuntu 26.04 VM -> `apt install ./kynex-desktop_*.deb` -> working Kynex session.

## Phase 12 — Hardening and 1.0
- [ ] Full audit, fuzz campaign, dependency review, penetration-style test of D-Bus surfaces
- [ ] Docs site, CODE_OF_CONDUCT, issue templates, legal review of branding/assets, public repo
- [ ] 1.0 release candidate on real hardware (multi-monitor, laptop, HiDPI), published benchmark vs Plasma and GNOME with methodology

## After 1.0 (deferred on purpose)
KynexOS distro: ISO build, installer, first-run setup, signed APT repository, branding, update channels.

## Backlog
Input methods polish, printing UI, backup tool, online accounts/cloud sync, calendar/weather/notes, gaming mode, tablet/touch polish, app store/Flatpak integration, crash reports (local only), onboarding tour.
