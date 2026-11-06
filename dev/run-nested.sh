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

# Start daemon in background
KYNEX_UPOWER_BUS=session "$BIN_DIR/kynex-daemon" 2>&1 | tee "$DAEMON_LOG" &
DAEMON_PID=\$!

# Start shell in foreground
"$BIN_DIR/kynex-shell" 2>&1 | tee "$SHELL_LOG" &
SHELL_PID=\$!

cleanup() {
  rm -f "$TMP_DIR/dbus-session-bus-address"
  kill "\$SHELL_PID" 2>/dev/null || true
  kill "\$DAEMON_PID" 2>/dev/null || true
  if [[ -n "\$FAKE_PID" ]]; then
    kill "\$FAKE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT TERM INT

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

exec dbus-run-session bash -c "
  exec kwin_wayland --width 1280 --height 720 \
    --no-lockscreen --no-global-shortcuts --exit-with-session \"$WRAPPER\" 2>&1 | tee \"$KWIN_LOG\"
"
