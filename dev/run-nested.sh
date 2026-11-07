#!/usr/bin/env bash
# Runs Kynex Shell in a nested KWin Wayland instance with private D-Bus and isolated user directories.
set -euo pipefail

# Ensure entire tree runs in its own session via setsid
if [[ "${KYNEX_IN_SESSION:-0}" != "1" ]]; then
  export KYNEX_IN_SESSION=1
  # Launch the nested session in a new session under setpriv --pdeathsig SIGTERM
  setpriv --pdeathsig SIGTERM setsid bash "$0" "$@" &
  SUPERVISED_CHILD=$!
  trap 'kill -TERM $SUPERVISED_CHILD 2>/dev/null || true; wait $SUPERVISED_CHILD 2>/dev/null || true; exit 130' INT
  trap 'kill -TERM $SUPERVISED_CHILD 2>/dev/null || true; wait $SUPERVISED_CHILD 2>/dev/null || true; exit 143' TERM
  trap 'kill -HUP $SUPERVISED_CHILD 2>/dev/null || true; wait $SUPERVISED_CHILD 2>/dev/null || true; exit 129' HUP
  wait "$SUPERVISED_CHILD" 2>/dev/null || true
  exit $?
fi

set +m  # Disable job control so background processes stay in the setsid process group
cd "$(dirname "$0")/.." || exit 1

ROOT_DIR="$(pwd)"
LOG_DIR="$ROOT_DIR/dev/logs"
TMP_DIR="$ROOT_DIR/dev/tmp"
mkdir -p "$LOG_DIR" "$TMP_DIR"

TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
KWIN_LOG="$LOG_DIR/kwin_${TIMESTAMP}.log"
SHELL_LOG="$LOG_DIR/shell_${TIMESTAMP}.log"
DAEMON_LOG="$LOG_DIR/daemon_${TIMESTAMP}.log"
BUSCTL_LOG="$LOG_DIR/busctl_${TIMESTAMP}.log"

PID_FILE="$TMP_DIR/nested-session.pids"

# 1. Clean stale processes from previous crashed or killed runs at startup
# Matches strictly against recorded PIDs whose /proc/<pid>/cmdline belongs to this workspace
clean_stale_processes() {
  if [[ -f "$PID_FILE" ]]; then
    echo "Checking for stale processes from previous run..."
    while read -r pid name || [[ -n "$pid" ]]; do
      [[ -z "$pid" ]] && continue
      if [[ -d "/proc/$pid" ]]; then
        local cmdline=""
        cmdline="$(tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null || true)"
        if echo "$cmdline" | grep -E 'kynex-shell|kynex-daemon|fake-upower|fake-networkmanager|kwin_wayland|dbus-daemon|busctl|session.sh|watchdog' | grep -q "$ROOT_DIR"; then
          echo "Cleaning stale process $pid ($name)..."
          kill -KILL "$pid" 2>/dev/null || true
        fi
      fi
    done < "$PID_FILE"
    rm -f "$PID_FILE"
  fi
}
clean_stale_processes

if [[ "${1:-}" == "--clean" || "${1:-}" == "--cleanup" ]]; then
  echo "Cleaned stale processes."
  exit 0
fi

# Record launcher PID
TARGET_PGID=$$
TARGET_SID=$(ps -o sid= -p $$ | tr -d ' ')
echo "$$ run-nested" > "$PID_FILE"

KWIN_PID=""
DBUS_PID=""

# Spawn watchdog child under setpriv --pdeathsig SIGTERM
# Plain shell (no file capabilities). When run-nested dies (even via kill -9),
# kernel delivers SIGTERM to watchdog, which tears down the entire session and process group.
# shellcheck disable=SC2016
setpriv --pdeathsig SIGTERM setsid bash -c '
  TARGET_PGID="$1"
  TARGET_SID="$2"
  WORKSPACE_ROOT="$3"
  trap "kill -TERM -\$TARGET_PGID 2>/dev/null || true; pkill -TERM -s \$TARGET_SID 2>/dev/null || true; sleep 0.2; kill -KILL -\$TARGET_PGID 2>/dev/null || true; pkill -KILL -s \$TARGET_SID 2>/dev/null || true; exit 0" TERM
  while true; do
    sleep 1
  done
' bash "$TARGET_PGID" "$TARGET_SID" "$ROOT_DIR" &
WATCHDOG_PID=$!
echo "$WATCHDOG_PID watchdog" >> "$PID_FILE"

cleanup_outer() {
  trap - EXIT INT TERM HUP
  echo "Stopping nested session and cleaning up children..."
  # Kill watchdog first to avoid race during normal/trapped teardown
  [[ -n "${WATCHDOG_PID:-}" ]] && kill -KILL "$WATCHDOG_PID" 2>/dev/null || true

  if [[ -f "$PID_FILE" ]]; then
    while read -r p _name || [[ -n "$p" ]]; do
      [[ -z "$p" || "$p" == "$$" ]] && continue
      kill -TERM "$p" 2>/dev/null || true
    done < "$PID_FILE"
    sleep 0.1
    while read -r p _name || [[ -n "$p" ]]; do
      [[ -z "$p" || "$p" == "$$" ]] && continue
      kill -KILL "$p" 2>/dev/null || true
    done < "$PID_FILE"
    rm -f "$PID_FILE"
  fi
  [[ -n "${KWIN_PID:-}" ]] && kill -KILL "$KWIN_PID" 2>/dev/null || true
  [[ -n "${DBUS_PID:-}" ]] && kill -KILL "$DBUS_PID" 2>/dev/null || true
  for p in $(pgrep -s "$TARGET_SID" 2>/dev/null || true); do
    if [[ "$p" != "$$" && "$p" != "${WATCHDOG_PID:-}" ]]; then
      kill -KILL "$p" 2>/dev/null || true
    fi
  done
  rm -f "$TMP_DIR/dbus-session-bus-address" "$TMP_DIR/dbus-address" "$TMP_DIR/bus.sock" "$TMP_DIR/wayland-display"
  rm -f "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"/wayland-[1-9]*
  exit 0
}
trap cleanup_outer EXIT INT TERM HUP

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
set +m

echo "\$\$ session-wrapper" >> "$PID_FILE"
echo "\$WAYLAND_DISPLAY" > "$TMP_DIR/wayland-display"

if [[ -f "$TMP_DIR/dbus-session-bus-address" ]]; then
  export DBUS_SESSION_BUS_ADDRESS="\$(cat "$TMP_DIR/dbus-session-bus-address")"
fi

# Start dev fake UPower service if enabled (default 1)
FAKE_PID=""
if [[ "\${KYNEX_FAKE_UPOWER:-1}" == "1" ]]; then
  setpriv --pdeathsig SIGKILL "$ROOT_DIR/dev/fake-upower.py" >/dev/null 2>&1 &
  FAKE_PID=\$!
  echo "\$FAKE_PID fake-upower" >> "$PID_FILE"
fi

# Start dev fake NetworkManager service if enabled (default 1)
FAKE_NM_PID=""
if [[ "\${KYNEX_FAKE_NM:-1}" == "1" ]]; then
  setpriv --pdeathsig SIGKILL "$ROOT_DIR/dev/fake-networkmanager.py" >/dev/null 2>&1 &
  FAKE_NM_PID=\$!
  echo "\$FAKE_NM_PID fake-networkmanager" >> "$PID_FILE"
fi

# Monitor D-Bus traffic on Daemon interface
busctl --user monitor org.kynexos.Daemon > "$BUSCTL_LOG" 2>&1 &
BUSCTL_PID=\$!
echo "\$BUSCTL_PID busctl-monitor" >> "$PID_FILE"

# Start daemon in background with pdeathsig SIGKILL
KYNEX_UPOWER_BUS=session KYNEX_NM_BUS=session KYNEX_ALLOW_TEST_FEEDER="${KYNEX_ALLOW_TEST_FEEDER:-0}" \
  setpriv --pdeathsig SIGKILL "$BIN_DIR/kynex-daemon" > "$DAEMON_LOG" 2>&1 &
DAEMON_PID=\$!
echo "\$DAEMON_PID kynex-daemon" >> "$PID_FILE"

# Start shell in background with pdeathsig SIGKILL
setpriv --pdeathsig SIGKILL "$BIN_DIR/kynex-shell" > "$SHELL_LOG" 2>&1 &
SHELL_PID=\$!
echo "\$SHELL_PID kynex-shell" >> "$PID_FILE"

# Load KWin window tracking script via org.kde.kwin.Scripting
(
  for _ in {1..50}; do
    if busctl --user status org.kde.KWin >/dev/null 2>&1; then
      break
    fi
    sleep 0.1
  done
  SCRIPT_PATH="$ROOT_DIR/kwin-scripts/kynex-windows/contents/code/main.js"
  if [[ -f "\$SCRIPT_PATH" ]] && busctl --user status org.kde.KWin >/dev/null 2>&1; then
    SCRIPT_ID=\$(busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting loadScript ss "\$SCRIPT_PATH" "kynex-windows" 2>/dev/null | awk '{print \$2}')
    if [[ -n "\$SCRIPT_ID" && "\$SCRIPT_ID" != "-1" ]]; then
      busctl --user call org.kde.KWin "/Scripting/Script\$SCRIPT_ID" org.kde.kwin.Script run >/dev/null 2>&1 || true
    fi
  fi
  LAUNCHER_SCRIPT_PATH="$ROOT_DIR/kwin-scripts/kynex-launcher/contents/code/main.js"
  if [[ -f "\$LAUNCHER_SCRIPT_PATH" ]] && busctl --user status org.kde.KWin >/dev/null 2>&1; then
    LAUNCHER_SCRIPT_ID=\$(busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting loadScript ss "\$LAUNCHER_SCRIPT_PATH" "kynex-launcher" 2>/dev/null | awk '{print \$2}')
    if [[ -n "\$LAUNCHER_SCRIPT_ID" && "\$LAUNCHER_SCRIPT_ID" != "-1" ]]; then
      busctl --user call org.kde.KWin "/Scripting/Script\$LAUNCHER_SCRIPT_ID" org.kde.kwin.Script run >/dev/null 2>&1 || true
    fi
  fi
) &

cleanup() {
  trap - EXIT INT TERM HUP
  [[ -n "\$SHELL_PID" ]] && kill -TERM "\$SHELL_PID" 2>/dev/null || true
  [[ -n "\$DAEMON_PID" ]] && kill -TERM "\$DAEMON_PID" 2>/dev/null || true
  [[ -n "\$FAKE_PID" ]] && kill -TERM "\$FAKE_PID" 2>/dev/null || true
  [[ -n "\$FAKE_NM_PID" ]] && kill -TERM "\$FAKE_NM_PID" 2>/dev/null || true
  [[ -n "\$BUSCTL_PID" ]] && kill -TERM "\$BUSCTL_PID" 2>/dev/null || true
  pkill -TERM -P \$\$ 2>/dev/null || true
  sleep 0.1
  [[ -n "\$SHELL_PID" ]] && kill -KILL "\$SHELL_PID" 2>/dev/null || true
  [[ -n "\$DAEMON_PID" ]] && kill -KILL "\$DAEMON_PID" 2>/dev/null || true
  [[ -n "\$FAKE_PID" ]] && kill -KILL "\$FAKE_PID" 2>/dev/null || true
  [[ -n "\$FAKE_NM_PID" ]] && kill -KILL "\$FAKE_NM_PID" 2>/dev/null || true
  [[ -n "\$BUSCTL_PID" ]] && kill -KILL "\$BUSCTL_PID" 2>/dev/null || true
  pkill -KILL -P \$\$ 2>/dev/null || true
  exit 0
}
trap cleanup EXIT INT TERM HUP

# Optional auto-exit timeout for automated testing
if [[ -n "\${KYNEX_TIMEOUT:-}" ]]; then
  ( sleep "\${KYNEX_TIMEOUT}"; kill -TERM "\$SHELL_PID" 2>/dev/null || true ) &
fi

# Optional extra dev client (e.g. dev/test-windows)
if [[ -n "\${KYNEX_EXTRA_CLIENT:-}" ]]; then
  (
    sleep 1.5
    \${KYNEX_EXTRA_CLIENT}
  ) &
fi

# Wait for shell
if [[ "\${KYNEX_KEEP_SESSION_ALIVE:-0}" == "1" ]]; then
  while true; do
    sleep 1
  done
else
  wait "\$SHELL_PID" || true
fi
cleanup
EOF
chmod +x "$WRAPPER"

echo "Starting nested session (logs in dev/logs)..."
export XDG_CONFIG_HOME="$TMP_DIR/config"
export XDG_DATA_HOME="$TMP_DIR/data"
export XDG_CACHE_HOME="$TMP_DIR/cache"
export XDG_STATE_HOME="$TMP_DIR/state"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME/applications" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"

export QT_QPA_PLATFORM=wayland
export KYNEX_STYLE_DIR="$ROOT_DIR/styles/default"
export RUST_LOG="${RUST_LOG:-info}"

# Start private session dbus-daemon directly as child with pdeathsig SIGKILL
DBUS_ADDR_FILE="$TMP_DIR/dbus-address"
DBUS_SOCK="$TMP_DIR/bus.sock"
rm -f "$DBUS_ADDR_FILE" "$DBUS_SOCK"
setpriv --pdeathsig SIGKILL dbus-daemon --session --address="unix:path=$DBUS_SOCK" --nofork --print-address > "$DBUS_ADDR_FILE" 2>&1 &
DBUS_PID=$!
echo "$DBUS_PID dbus-daemon" >> "$PID_FILE"

for _ in {1..50}; do
  if [[ -s "$DBUS_ADDR_FILE" ]]; then
    break
  fi
  sleep 0.05
done

DBUS_SESSION_BUS_ADDRESS="$(head -n 1 "$DBUS_ADDR_FILE")"
export DBUS_SESSION_BUS_ADDRESS
echo "$DBUS_SESSION_BUS_ADDRESS" > "$TMP_DIR/dbus-session-bus-address"

# Start kwin_wayland directly as child with pdeathsig SIGKILL
setpriv --pdeathsig SIGKILL kwin_wayland --width 1280 --height 720 \
  --no-lockscreen --no-global-shortcuts --exit-with-session "$WRAPPER" > "$KWIN_LOG" 2>&1 &
KWIN_PID=$!
echo "$KWIN_PID kwin_wayland" >> "$PID_FILE"

wait "$KWIN_PID" || true
cleanup_outer
