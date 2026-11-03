#!/usr/bin/env bash
# Installs system packages for Kynex development on Ubuntu 26.04. Run with sudo, once.
# Packages missing from apt are reported and skipped (names can differ between releases).
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "Run with sudo: sudo $0" >&2
  exit 1
fi

export DEBIAN_FRONTEND=noninteractive

PACKAGES=(
  # toolchain
  build-essential git gh curl ca-certificates jq cmake ninja-build pkg-config clang lld mold libclang-dev shellcheck
  # qt 6 + qml
  qt6-base-dev qt6-declarative-dev qt6-declarative-dev-tools qt6-wayland qt6-wayland-dev qt6-svg-dev
  qt6-tools-dev qt6-tools-dev-tools qt6-l10n-tools
  qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts qml6-module-qtquick-window
  qml6-module-qtquick-templates qml6-module-qtqml qml6-module-qtqml-workerscript qml6-module-qttest
  # compositor + layer shell
  kwin-wayland kwin-wayland-backend-virtual kwin-wayland-backend-x11 xwayland
  layer-shell-qt liblayershellqtinterface-dev
  libwayland-dev wayland-protocols libxkbcommon-dev
  # system services we reuse
  dbus dbus-user-session libdbus-1-dev d-spy upower network-manager bluez udisks2 polkitd
  pipewire wireplumber libpipewire-0.3-dev
  # tools for tests, profiling, benchmarks
  hyperfine sysstat smem valgrind heaptrack linux-tools-generic desktop-file-utils xdg-utils
  # vm niceties
  openssh-server spice-vdagent
)

apt-get update

INSTALL=()
MISSING=()
for p in "${PACKAGES[@]}"; do
  if apt-cache show "$p" >/dev/null 2>&1; then
    INSTALL+=("$p")
  else
    MISSING+=("$p")
  fi
done

echo "Installing ${#INSTALL[@]} packages..."
apt-get install -y --no-install-recommends "${INSTALL[@]}"

if ((${#MISSING[@]})); then
  echo
  echo "NOT FOUND in apt (skipped) — tell the agent/assistant to resolve these names:"
  printf '  %s\n' "${MISSING[@]}"
fi
echo "Done. Next: ./scripts/bootstrap-user.sh (no sudo)."
