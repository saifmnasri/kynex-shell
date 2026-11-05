# Kynex Shell

<div align="center">

**A fast, memory-safe, and modular desktop shell for Linux**  
*Built on KWin 6 (Wayland), powered by Rust, styled with modern Qt Quick, and engineered for enterprise-grade speed, polish, and security.*

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.99+-orange.svg)](rust-toolchain.toml)
[![Qt](https://img.shields.io/badge/Qt-6.4+-green.svg)](https://www.qt.io/)
[![Wayland](https://img.shields.io/badge/Wayland-KWin%206-purple.svg)](https://invent.kde.org/plasma/kwin)
[![Security Policy](https://img.shields.io/badge/Security-Policy-brightgreen.svg)](SECURITY.md)

</div>

---

## Overview

**Kynex Shell** is the desktop environment shell for KynexOS, engineered from the ground up to outperform traditional desktop environments in memory footprint, input responsiveness, and architectural cleanliness.

Instead of reinventing the display compositor, Kynex Shell delegates low-level Wayland composition, DRM/KMS hardware rendering, and input handling to **KWin 6**, focusing entirely on building a world-class user experience, robust system integration, and a decoupled, swappable style engine.

### Core Philosophy

1. **Rust for Logic, QML for Presentation**  
   All state management, D-Bus communication, system monitoring, and background tasks run in memory-safe Rust. The UI layer (Qt Quick / QML) remains strictly presentational and contains zero business logic.
2. **KWin First**  
   Window management, workspace transitions, and compositor effects are delegated directly to KWin 6 via native Wayland protocols (`wlr-layer-shell`, `xdg-shell`) and scripting interfaces.
3. **Decoupled Style Packs**  
   Core code contains **zero hardcoded layouts, colors, margins, or fonts**. The entire visual identity and layout hierarchy are loaded from style packs (`styles/<id>/`), allowing users to switch between dock, taskbar, or overview archetypes without touching core binaries.
4. **Security by Default**  
   Strict path traversal prevention, symlink escape rejection, isolated background processes for thumbnail generation, hardened systemd user services, and zero ambient privileges.
5. **Clean System Service Reuse**  
   Reuses standard, battle-tested Linux system daemons (UPower, NetworkManager, BlueZ, PipeWire/WirePlumber, logind, and polkit) rather than maintaining duplicate stacks.

---

## Architecture

```
                  Applications (Wayland Clients)
                               │
                ┌──────────────▼──────────────┐
                │      KWin 6 Compositor      │◄─── Window Tracking Script
                └──────────────┬──────────────┘
                               │ wlr-layer-shell
                ┌──────────────▼──────────────┐
                │         kynex-shell         │
                │  (Qt Quick UI + CXX-Qt FFI) │
                └──────────────▲──────────────┘
                               │ D-Bus (Private IPC)
                ┌──────────────▼──────────────┐
                │         kynex-daemon        │
                │    (Rust, Tokio + ZBus)     │
                └──────────────┬──────────────┘
                               │
      ┌────────────┬───────────┼───────────┬────────────┐
      ▼            ▼           ▼           ▼            ▼
   UPower   NetworkManager   BlueZ     PipeWire      logind
  (Battery)   (Networking) (Bluetooth) (Audio/Vol) (Sessions)
```

### Workspace Structure

The project is organized as a Cargo workspace with clean separation of concerns:

| Crate | Role |
| :--- | :--- |
| [`kynex-common`](crates/kynex-common) | Shared data structures, constants, and configuration schemas (zero I/O dependencies). |
| [`kynex-ipc`](crates/kynex-ipc) | Type-safe D-Bus interface definitions and serialization contracts shared between daemon and clients. |
| [`kynex-style`](crates/kynex-style) | Style pack parser, token engine, and security validator (with validation tests against path traversal and malicious payloads). |
| [`kynex-daemon`](crates/kynex-daemon) | Background system service managing hardware indicators, power, networking, audio, and notification routing. |
| [`kynex-shell`](crates/kynex-shell) | Graphical shell process. Bridges Rust and Qt 6 via `cxx-qt`, providing the `LayerSurface` C++ shim for Wayland layer-shell windows. |

---

## Swappable Style Packs

Kynex Shell decouples presentation from application logic through **Style Packs**:

```
styles/<pack-id>/
├── style.toml          # Pack manifest (metadata, archetype, asset declarations)
├── tokens.toml         # Design tokens (colors, radii, spacing, typography, motion)
├── Shell.qml           # Primary shell entry point
└── surfaces/           # Surface components (Panels, Docks, Launchers, Popups)
```

### Supported Layout Archetypes

- **`dock`**: Minimal top status bar with a centered floating dock, application grid launcher, and swipeable workspaces.
- **`taskbar`**: Traditional unified bottom taskbar with pinned launchers, active window grouping, system tray, and quick settings flyout.
- **`overview`**: Keyboard-driven top bar paired with a full-screen interactive workspace and window overview.
- **`custom`**: Free-form user layouts combining custom layer-shell surfaces.

---

## Getting Started

### Prerequisites

- **OS**: Modern Linux distribution (Ubuntu 24.04/26.04 LTS, Debian 13+, Fedora 40+, Arch Linux).
- **Display Server**: Wayland with KWin 6 (`kwin-wayland`).
- **Rust**: Version 1.85+ (pinned via `rust-toolchain.toml`).
- **C++ / Qt**: Clang or GCC supporting C++20, CMake 3.22+, Qt 6.4+ (Declarative, Quick, QuickControls2), and `LayerShellQt`.

#### Installing Dependencies (Ubuntu / Debian)

You can bootstrap required system dependencies using the included setup script:

```bash
sudo ./scripts/bootstrap-system.sh
./scripts/bootstrap-user.sh
```

Or install them manually:

```bash
sudo apt update && sudo apt install -y \
  build-essential cmake clang pkg-config \
  qt6-base-dev qt6-declarative-dev qml6-module-qtquick-controls \
  liblayershellqt-dev kwin-wayland libwayland-dev \
  libdbus-1-dev libglib2.0-dev libudev-dev
```

### Building the Workspace

Clone the repository and build with Cargo:

```bash
git clone https://github.com/saifmnasri/kynex-shell.git
cd kynex-shell

# Build all workspace crates
cargo build --workspace
```

### Running Tests and Quality Gates

Before any code is committed, run the complete verification gate:

```bash
# Run unit and integration tests
cargo nextest run --workspace

# Run the complete verification suite (fmt, clippy, tests, deny, audit, linter)
./scripts/verify.sh
```

---

## Development & Running

### Running in a Nested Wayland Session

You can run Kynex Shell inside a nested KWin Wayland window directly on your existing desktop without replacing your current session:

```bash
# Launches an isolated nested session with dedicated D-Bus and private XDG paths
./dev/run-nested.sh
```

### Automated Headless Smoke Test

To verify that KWin Wayland and Kynex Shell start, negotiate layer-shell surfaces, and render without QML warnings in headless environments (e.g. CI):

```bash
./scripts/smoke-test.sh
```

---

## Security

Security is a primary design goal:

- **Strict Validation**: All external input (style manifests, tokens, desktop files, D-Bus payloads) is validated and sanitised against path traversal, symlink escapes, and resource exhaustion.
- **Memory Safety**: Core logic is implemented in safe Rust with strict compiler lints (`[lints] workspace = true`, `clippy::pedantic`).
- **Vulnerability Disclosure**: If you discover a security vulnerability, please refer to our [Security Policy](SECURITY.md) for instructions on confidential disclosure.

---

## Contributing

Contributions, bug reports, and suggestions are welcome!

1. Read [`CONTRIBUTING.md`](CONTRIBUTING.md) for coding standards, Conventional Commit guidelines, and PR requirements.
2. Ensure `./scripts/verify.sh` passes completely with zero errors and zero warnings.
3. Open a Pull Request on GitHub.

---

## License

Kynex Shell is open-source software licensed under the **[GNU General Public License v3.0 or later](LICENSE)** (`GPL-3.0-or-later`).

- **Copyleft Protection**: No third-party or corporate entity can make proprietary, closed-source forks of this desktop environment.
- **Relicensing & Commercial Rights**: All relicensing and commercial rights are reserved by the author, **Saifeddine Mnasri**. External contributions are accepted pursuant to the Contributor License Agreement in [`CONTRIBUTING.md`](CONTRIBUTING.md).
- **Dependency Philosophy**: The project strictly prefers permissive licenses (**MIT**, **BSD**, **Apache-2.0**, **ISC**, **Zlib**) for third-party libraries and dependencies. All dependencies must be GPL-3.0-compatible; **GPL-2.0-only** is strictly banned.
- **Qt 6 Framework**: Qt 6 is dynamically linked under the terms of LGPL-3.0.
- **KWin Compositor**: KWin 6 runs in a separate process as an independent Wayland compositor and interacts purely via standard Wayland and D-Bus IPC protocols.
- **Assets**: Third-party and original asset licenses are documented in [`assets/LICENSES.md`](assets/LICENSES.md).
