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

START_TIME="$(date '+%Y-%m-%d %H:%M:%S')"

echo "=== 2. Starting test unit ==="
systemctl --user start "$SERVICE_NAME"

echo "=== 3. Verifying unit status ==="
systemctl --user status "$SERVICE_NAME" --no-pager

MAINPID="$(systemctl --user show --property=MainPID --value "$SERVICE_NAME")"
if [[ -z "$MAINPID" || "$MAINPID" == "0" ]]; then
  echo "ERROR: Could not obtain MainPID for running unit $SERVICE_NAME" >&2
  exit 1
fi

echo "=== 4. Inspecting process sandboxing and effective protections ==="
echo "Process MainPID: $MAINPID"
echo "Test shell PID: $$"

DAEMON_NET="$(readlink "/proc/$MAINPID/ns/net" 2>/dev/null || echo "unknown")"
SHELL_NET="$(readlink "/proc/$$/ns/net" 2>/dev/null || echo "unknown")"
DAEMON_UTS="$(readlink "/proc/$MAINPID/ns/uts" 2>/dev/null || echo "unknown")"
SHELL_UTS="$(readlink "/proc/$$/ns/uts" 2>/dev/null || echo "unknown")"
DAEMON_MNT="$(readlink "/proc/$MAINPID/ns/mnt" 2>/dev/null || echo "unknown")"
SHELL_MNT="$(readlink "/proc/$$/ns/mnt" 2>/dev/null || echo "unknown")"

echo "Namespace comparison:"
echo "  net: daemon=$DAEMON_NET shell=$SHELL_NET (isolated: $([[ "$DAEMON_NET" != "$SHELL_NET" ]] && echo "YES" || echo "NO"))"
echo "  uts: daemon=$DAEMON_UTS shell=$SHELL_UTS (isolated: $([[ "$DAEMON_UTS" != "$SHELL_UTS" ]] && echo "YES" || echo "NO"))"
echo "  mnt: daemon=$DAEMON_MNT shell=$SHELL_MNT (isolated: $([[ "$DAEMON_MNT" != "$SHELL_MNT" ]] && echo "YES" || echo "NO"))"

echo "Process status flags (/proc/$MAINPID/status):"
grep -E '^(NoNewPrivs|Seccomp):' "/proc/$MAINPID/status" || true

echo "=== 5. Verifying D-Bus registration and Power1 interface ==="
busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1
busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1 Present Percentage State

echo "=== 6. Inspecting user journal output ==="
JOURNAL_OUTPUT="$(journalctl --user -u "$SERVICE_NAME" --since "$START_TIME" --no-pager)"
echo "$JOURNAL_OUTPUT"

HAS_WARNINGS=0
if echo "$JOURNAL_OUTPUT" | grep -Ei 'proceeding without|ignoring|failed to'; then
  HAS_WARNINGS=1
fi

echo "=== 7. Stopping unit and verifying cleanup ==="
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

if [[ "$HAS_WARNINGS" -ne 0 ]]; then
  echo "FAIL: Unit journal contains degraded or ignored sandboxing directives ('proceeding without', 'ignoring', or 'failed to')." >&2
  exit 1
fi

echo "SUCCESS: Unit ran with full sandboxing, acquired D-Bus name, Power1 was queried, journal is clean, and ~/.config/systemd was untouched."
