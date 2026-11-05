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

echo "Building Kynex workspace..."
cargo build --workspace

WRAPPER="$TMP_DIR/session.sh"
cat << EOF > "$WRAPPER"
#!/usr/bin/env bash
set -euo pipefail

# Start daemon in background
"$ROOT_DIR/target/debug/kynex-daemon" 2>&1 | tee "$DAEMON_LOG" &
DAEMON_PID=\$!

# Start shell in foreground
"$ROOT_DIR/target/debug/kynex-shell" 2>&1 | tee "$SHELL_LOG" &
SHELL_PID=\$!

cleanup() {
  kill "\$SHELL_PID" 2>/dev/null || true
  kill "\$DAEMON_PID" 2>/dev/null || true
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

exec dbus-run-session bash -c "
  exec kwin_wayland --width 1280 --height 720 \
    --no-lockscreen --no-global-shortcuts --exit-with-session \"$WRAPPER\" 2>&1 | tee \"$KWIN_LOG\"
"
