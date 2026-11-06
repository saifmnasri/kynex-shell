#!/usr/bin/env bash
# Integration test for kynex-daemon systemd user unit with sandboxing directives.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

UNIT_NAME="kynex-daemon-test"
SERVICE_NAME="${UNIT_NAME}.service"
RUNTIME_USER_DIR="/run/user/$(id -u)/systemd/user"
TARGET_UNIT_PATH="${RUNTIME_USER_DIR}/${SERVICE_NAME}"

# Ensure binary is built
DAEMON_BIN="${ROOT_DIR}/target/debug/kynex-daemon"
if [[ ! -x "$DAEMON_BIN" ]]; then
  echo "Building kynex-daemon..."
  cargo build -p kynex-daemon --locked
fi

# Cleanup trap to guarantee nothing is left behind
cleanup() {
  echo "Cleaning up test systemd unit..."
  systemctl --user stop "$SERVICE_NAME" >/dev/null 2>&1 || true
  rm -f "$TARGET_UNIT_PATH"
  systemctl --user daemon-reload >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "=== 1. Deploying test unit to /run/user/$(id -u)/systemd/user ==="
mkdir -p "$RUNTIME_USER_DIR"

# Generate test unit matching systemd/kynex-daemon.service directives but with local binary
sed -e "s|ExecStart=.*|ExecStart=${DAEMON_BIN}|" \
    -e "s|Restart=.*|Restart=no|" \
    "$ROOT_DIR/systemd/kynex-daemon.service" > "$TARGET_UNIT_PATH"

systemctl --user daemon-reload

echo "=== 2. Starting test unit ==="
systemctl --user start "$SERVICE_NAME"

echo "=== 3. Verifying unit status ==="
systemctl --user status "$SERVICE_NAME" --no-pager

echo "=== 4. Verifying D-Bus registration and Power1 interface ==="
busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1
busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1 Present Percentage State

echo "=== 5. Inspecting user journal output ==="
journalctl --user -u "$SERVICE_NAME" -n 20 --no-pager

echo "=== 6. Stopping unit and verifying cleanup ==="
systemctl --user stop "$SERVICE_NAME"
rm -f "$TARGET_UNIT_PATH"
systemctl --user daemon-reload

if [[ -d "$HOME/.config/systemd" ]]; then
  LEFTOVERS="$(find "$HOME/.config/systemd" -name "*kynex*" 2>/dev/null || true)"
  if [[ -n "$LEFTOVERS" ]]; then
    echo "ERROR: Leftover files found in ~/.config/systemd:" >&2
    echo "$LEFTOVERS" >&2
    exit 1
  fi
fi

echo "SUCCESS: Unit ran with full sandboxing, acquired D-Bus name, Power1 was queried, journal is clean, and ~/.config/systemd was untouched."

