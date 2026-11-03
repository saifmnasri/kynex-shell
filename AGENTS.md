# KynexOS / Kynex Shell — Agent Instructions

## Mission
Build a complete desktop environment, A to Z, that beats KDE Plasma and GNOME in speed, weight, polish and security, built like an enterprise product. Target: Ubuntu 26.04 LTS. Kynex Shell = our own shell on top of KWin 6 (Wayland). Scope order: the desktop environment first; a full distro ISO/installer only after v1.0.

## Non-negotiables
1. **Verify, don't assume.** Never say "done", "fixed" or "correct" without evidence. Run `./scripts/verify.sh` and show the result. Perfect code cannot be promised; proven behavior can.
2. **Double-check everything.** Before reporting: re-read your own diff as a hostile reviewer; check every API against the docs or installed headers for the pinned version; run tests; run `/audit` at the end of each phase. List what you verified and what you could NOT verify.
3. **Never invent.** No guessed APIs, crate names, versions, flags or package names. If unsure, search/read the docs or ask.
4. **KWin first.** If KWin (or a standard system service) already does it, use it and build only the UI. See `docs/KWIN_DELEGATION.md`.
5. **Rust for all logic.** QML is presentation only. C++ only for the minimal shim/FFI.
6. **Security by default.** See `.agents/rules/02-security.md` and `docs/THREAT_MODEL.md`.
7. **Licenses.** Our code is MIT. Prefer MIT/BSD/Apache/ISC/Zlib dependencies; anything else needs a justified exception. See `docs/LICENSING.md`.
8. **Small, reviewable steps.** One ROADMAP item at a time; stop when it passes the gates and report.
9. **Style-ready from day one.** Core provides logic, data models and surface toolkit; *style packs* provide all layout and look. Never hard-code a layout or look in core. See `docs/STYLES.md`.
10. **No third-party names or assets.** No trademarked product names, logos, icons, fonts, sounds or wallpapers anywhere (code, UI, docs, file names). Original work only. See `.agents/rules/08-styles-and-trademarks.md`.

## Stack (change only via docs/DECISIONS.md)
Rust workspace (`crates/`); Qt 6 Qt Quick + Controls 2 (Basic style, own components); cxx-qt (exact pin); LayerShellQt via minimal C++ shim; zbus + tokio; KWin JS script for window data; systemd user services; UPower, NetworkManager, BlueZ, udisks2, PipeWire/WirePlumber, polkit, logind reused.

## Layout
```
crates/kynex-common/   shared types, config schema, constants (no I/O)
crates/kynex-ipc/      D-Bus interface definitions shared by daemon and clients
crates/kynex-daemon/   services (one module per service, trait + impl)
crates/kynex-shell/    cxx-qt app, cpp/ shim, qml/
crates/kynex-style/    style-pack manifest, tokens, loader/validator, switching
crates/kynex-*/        future: -files-core, -apps-index, -settings
styles/                built-in style packs (default first)
kwin-scripts/  packaging/  systemd/  dev/  scripts/  templates/  docs/  .agents/  .github/  .githooks/
```

## Commands
`./scripts/verify.sh` (full gate) · `./scripts/verify.sh --fast` (pre-push) · `./scripts/audit.sh` (deep audit) · `./dev/run-nested.sh` (run) · `./scripts/check-env.sh` (environment) · `cargo nextest run --workspace`

## Workflow
Read `SETUP.md` (first time), `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/STYLES.md`, `docs/FEATURES.md`, then do the first unchecked step. Use branches and Conventional Commits (`docs/GIT_WORKFLOW.md`). Tick the ROADMAP box and log decisions in `docs/DECISIONS.md` as part of the task.

## Definition of done
- `./scripts/verify.sh` passes (fmt, clippy `-D warnings`, tests, deny, audit, lints).
- New behavior has tests; bugs get a regression test first.
- Runs in nested KWin (`/run-nested`) when UI-related.
- Docs updated (ROADMAP, DECISIONS, DBUS_API, ARCHITECTURE as needed). Committed on a branch with a Conventional Commit message.

## Final reply format
**Changed** (files) · **Verified** (commands + results) · **Not verified / risks** · **How to test** · **Next step**.
