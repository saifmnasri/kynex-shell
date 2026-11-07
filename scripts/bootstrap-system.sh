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
  kwin-wayland xwayland
  layer-shell-qt liblayershellqtinterface-dev
  libwayland-dev wayland-protocols libxkbcommon-dev
  # system services we reuse
  dbus dbus-user-session libdbus-1-dev d-spy upower network-manager bluez udisks2 polkitd
  pipewire wireplumber libpipewire-0.3-dev
  # tools for tests, profiling, benchmarks
  hyperfine sysstat smem valgrind heaptrack desktop-file-utils xdg-utils python3-pil fonts-noto-core
  # vm niceties
  openssh-server spice-vdagent
)

# Ensure system clock matches hardware clock to avoid "Release file is not valid yet" errors
if command -v timedatectl >/dev/null 2>&1; then
  rtc_time="$(timedatectl status 2>/dev/null | grep 'RTC time:' | awk '{print $3, $4}')"
  if [[ -n "$rtc_time" ]]; then
    timedatectl set-ntp false 2>/dev/null || true
    timedatectl set-time "$rtc_time" 2>/dev/null || date -s "$rtc_time" 2>/dev/null || true
    timedatectl set-ntp true 2>/dev/null || true
  fi
fi

wait_for_dpkg_lock() {
  while fuser /var/lib/dpkg/lock-frontend >/dev/null 2>&1 || fuser /var/lib/dpkg/lock >/dev/null 2>&1 || fuser /var/lib/apt/lists/lock >/dev/null 2>&1; do
    echo "Waiting for other apt/dpkg process to release lock..."
    sleep 3
  done
}

wait_for_dpkg_lock
apt-get -o Acquire::Check-Date=false -o Acquire::Check-Valid-Until=false update

INSTALL=()
MISSING=()
for p in "${PACKAGES[@]}"; do
  if dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "ok installed"; then
    continue
  fi
  cand="$(apt-cache policy "$p" 2>/dev/null | grep 'Candidate:' | awk '{print $2}')"
  if [[ -n "$cand" && "$cand" != "(none)" ]]; then
    INSTALL+=("$p")
  else
    MISSING+=("$p")
  fi
done

if ((${#INSTALL[@]})); then
  echo "Installing ${#INSTALL[@]} packages..."
  wait_for_dpkg_lock
  if ! apt-get -o Acquire::Check-Date=false -o Acquire::Check-Valid-Until=false install -y --no-install-recommends "${INSTALL[@]}"; then
    echo "Batch installation encountered an issue; installing packages individually..."
    for p in "${INSTALL[@]}"; do
      wait_for_dpkg_lock
      apt-get -o Acquire::Check-Date=false -o Acquire::Check-Valid-Until=false install -y --no-install-recommends "$p" || {
        echo "WARNING: Skipping package $p which failed to install" >&2
      }
    done
  fi
else
  echo "All required system packages are already installed."
fi

if ((${#MISSING[@]})); then
  echo
  echo "NOT FOUND in apt (skipped) — tell the agent/assistant to resolve these names:"
  printf '  %s\n' "${MISSING[@]}"
fi
echo "Done. Next: ./scripts/bootstrap-user.sh (no sudo)."
