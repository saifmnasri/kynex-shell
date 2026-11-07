#!/usr/bin/env bash
# Runs Kynex Shell in a nested KWin Wayland instance with private D-Bus and isolated user directories.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

ROOT_DIR="$(pwd)"
LOG_DIR="$ROOT_DIR/dev/logs"
TMP_DIR="$ROOT_DIR/dev/tmp"
mkdir -p "$LOG_DIR" "$TMP_DIR"

TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
KWIN_LOG="$LOG_DIR/kwin_${TIMESTAMP}.log"
SHELL_LOG="$LOG_DIR/shell_${TIMESTAMP}.log"
DAEMON_LOG="$LOG_DIR/daemon_${TIMESTAMP}.log"

BUILD_PROFILE="${KYNEX_BUILD_PROFILE:-debug}"
CARGO_ARGS=("--workspace")
if [[ "$BUILD_PROFILE" == "release" ]]; then
  CARGO_ARGS+=("--release" "--locked")
fi

echo "Building Kynex workspace ($BUILD_PROFILE)..."
cargo build "${CARGO_ARGS[@]}"

BIN_DIR="$ROOT_DIR/target/$BUILD_PROFILE"

WRAPPER="$TMP_DIR/session.sh"
PID_FILE="$TMP_DIR/session.pids"
rm -f "$PID_FILE"

cat << EOF > "$WRAPPER"
#!/usr/bin/env bash
set -euo pipefail

# Export session bus address for dev inspection tools
echo "\$DBUS_SESSION_BUS_ADDRESS" > "$TMP_DIR/dbus-session-bus-address"

# Start dev fake UPower service if enabled (default 1)
FAKE_PID=""
if [[ "\${KYNEX_FAKE_UPOWER:-1}" == "1" ]]; then
  "$ROOT_DIR/dev/fake-upower.py" >/dev/null 2>&1 &
  FAKE_PID=\$!
fi

# Start dev fake NetworkManager service if enabled (default 1)
FAKE_NM_PID=""
if [[ "\${KYNEX_FAKE_NM:-1}" == "1" ]]; then
  "$ROOT_DIR/dev/fake-networkmanager.py" >/dev/null 2>&1 &
  FAKE_NM_PID=\$!
fi

# Start daemon in background (direct redirection guarantees \$! is the actual daemon PID, not tee)
KYNEX_UPOWER_BUS=session KYNEX_NM_BUS=session "$BIN_DIR/kynex-daemon" > "$DAEMON_LOG" 2>&1 &
DAEMON_PID=\$!

# Start shell in background
"$BIN_DIR/kynex-shell" > "$SHELL_LOG" 2>&1 &
SHELL_PID=\$!

# Save active PIDs for outer process-group cleanup
echo "\$SHELL_PID \$DAEMON_PID \$FAKE_PID \$FAKE_NM_PID" > "$PID_FILE"

cleanup() {
  trap - EXIT INT TERM HUP
  rm -f "$TMP_DIR/dbus-session-bus-address" "$PID_FILE"
  # Terminate tracked children directly
  [[ -n "\$SHELL_PID" ]] && kill -TERM "\$SHELL_PID" 2>/dev/null || true
  [[ -n "\$DAEMON_PID" ]] && kill -TERM "\$DAEMON_PID" 2>/dev/null || true
  [[ -n "\$FAKE_PID" ]] && kill -TERM "\$FAKE_PID" 2>/dev/null || true
  [[ -n "\$FAKE_NM_PID" ]] && kill -TERM "\$FAKE_NM_PID" 2>/dev/null || true
  # Terminate any remaining child processes of this wrapper
  pkill -TERM -P \$\$ 2>/dev/null || true
  sleep 0.1
  [[ -n "\$SHELL_PID" ]] && kill -KILL "\$SHELL_PID" 2>/dev/null || true
  [[ -n "\$DAEMON_PID" ]] && kill -KILL "\$DAEMON_PID" 2>/dev/null || true
  [[ -n "\$FAKE_PID" ]] && kill -KILL "\$FAKE_PID" 2>/dev/null || true
  [[ -n "\$FAKE_NM_PID" ]] && kill -KILL "\$FAKE_NM_PID" 2>/dev/null || true
  pkill -KILL -P \$\$ 2>/dev/null || true
}
trap cleanup EXIT INT TERM HUP

# Optional auto-exit timeout for automated testing
if [[ -n "\${KYNEX_TIMEOUT:-}" ]]; then
  ( sleep "\$KYNEX_TIMEOUT"; kill "\$SHELL_PID" 2>/dev/null || true ) &
fi

# Wait for shell
wait "\$SHELL_PID" || true
EOF
chmod +x "$WRAPPER"

echo "Starting nested session (logs in dev/logs)..."
# Keep host XDG_RUNTIME_DIR and WAYLAND_DISPLAY so nested KWin can connect to parent compositor socket.
# Isolate user configuration, data, cache, and state under dev/tmp/.
export XDG_CONFIG_HOME="$TMP_DIR/config"
export XDG_DATA_HOME="$TMP_DIR/data"
export XDG_CACHE_HOME="$TMP_DIR/cache"
export XDG_STATE_HOME="$TMP_DIR/state"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"

export QT_QPA_PLATFORM=wayland
export KYNEX_STYLE_DIR="$ROOT_DIR/styles/default"
export RUST_LOG="${RUST_LOG:-info}"

KWIN_RUNNER_PID=""

cleanup_outer() {
  trap - EXIT INT TERM HUP
  echo "Stopping nested session and cleaning up children..."
  if [[ -f "$PID_FILE" ]]; then
    SESSION_PIDS="$(cat "$PID_FILE" 2>/dev/null || true)"
    for p in $SESSION_PIDS; do
      kill -TERM "$p" 2>/dev/null || true
    done
    sleep 0.1
    for p in $SESSION_PIDS; do
      kill -KILL "$p" 2>/dev/null || true
    done
    rm -f "$PID_FILE"
  fi
  if [[ -n "$KWIN_RUNNER_PID" ]]; then
    kill -TERM "$KWIN_RUNNER_PID" 2>/dev/null || true
    sleep 0.1
    kill -KILL "$KWIN_RUNNER_PID" 2>/dev/null || true
  fi
  pkill -TERM -P $$ 2>/dev/null || true
}
trap cleanup_outer EXIT INT TERM HUP

dbus-run-session bash -c "
  kwin_wayland --width 1280 --height 720 \
    --no-lockscreen --no-global-shortcuts --exit-with-session \"$WRAPPER\" > \"$KWIN_LOG\" 2>&1
" &
KWIN_RUNNER_PID=$!

wait "$KWIN_RUNNER_PID" || true
