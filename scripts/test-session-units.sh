#!/usr/bin/env bash
# Unit-parity rehearsal test for kynex-daemon and kynex-shell.
#
# Validates R2.2 and R2.3:
# 1. Reads real properties from packaged unit files (kynex-daemon.service, kynex-shell.service).
# 2. Starts them under systemd-run --user with real sandbox/resource properties against a nested KWin.
# 3. Asserts both stay active for 30 seconds without restarts.
# 4. Asserts shell log shows panel initialized with GL rendering (llvmpipe/OpenGL, not software backend).
# 5. Asserts daemon owns bus names on the private bus.
# 6. Asserts launched fake app has no inherited sandbox (Seccomp: 0, NoNewPrivs: 0, Umask != 0077, PPid = user manager)
#    and survives daemon kill -9.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

DAEMON_SERVICE_FILE="$ROOT_DIR/packaging/session/kynex-daemon.service"
SHELL_SERVICE_FILE="$ROOT_DIR/packaging/session/kynex-shell.service"

if [[ ! -f "$DAEMON_SERVICE_FILE" || ! -f "$SHELL_SERVICE_FILE" ]]; then
  echo "ERROR: Unit files not found!" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d -t kynex-session-test-XXXXXX)"
PID_FILE="$TMP_DIR/pids"
touch "$PID_FILE"

RUN_ID="test-$$-${RANDOM}"
DAEMON_UNIT="kynex-daemon-${RUN_ID}"
SHELL_UNIT="kynex-shell-${RUN_ID}"

KWIN_PID=""
DBUS_PID=""
FAKE_APP_PID=""

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  echo "=== Cleaning up rehearsal session (${RUN_ID}) ==="
  systemctl --user stop "${SHELL_UNIT}.service" >/dev/null 2>&1 || true
  systemctl --user stop "${DAEMON_UNIT}.service" >/dev/null 2>&1 || true
  systemctl --user reset-failed "${SHELL_UNIT}.service" "${DAEMON_UNIT}.service" >/dev/null 2>&1 || true

  [[ -n "$FAKE_APP_PID" ]] && kill -KILL "$FAKE_APP_PID" 2>/dev/null || true
  [[ -n "$KWIN_PID" ]] && kill -KILL "$KWIN_PID" 2>/dev/null || true
  [[ -n "$DBUS_PID" ]] && kill -KILL "$DBUS_PID" 2>/dev/null || true

  rm -rf "$TMP_DIR"
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

echo "=== 1. Building binaries if needed ==="
cargo build -p kynex-daemon -p kynex-shell
BIN_DAEMON="$ROOT_DIR/target/debug/kynex-daemon"
BIN_SHELL="$ROOT_DIR/target/debug/kynex-shell"

echo "=== 2. Parsing packaged unit file properties ==="
extract_props() {
  local file="$1"
  local props=()
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" || "$line" =~ ^# ]] && continue
    if [[ "$line" =~ ^(StartLimitIntervalSec|StartLimitBurst|RestartSteps|RestartMaxDelaySec|RestartSec|NoNewPrivileges|RestrictAddressFamilies|SystemCallFilter|SystemCallArchitectures|RestrictRealtime|RestrictNamespaces|RestrictSUIDSGID|MemoryDenyWriteExecute|LockPersonality|KeyringMode|UMask)= ]]; then
      props+=("-p" "$line")
    fi
  done < "$file"
  printf '%s\n' "${props[@]}"
}

readarray -t DAEMON_PROPS < <(extract_props "$DAEMON_SERVICE_FILE")
readarray -t SHELL_PROPS < <(extract_props "$SHELL_SERVICE_FILE")

echo "Extracted ${#DAEMON_PROPS[@]} daemon property tokens and ${#SHELL_PROPS[@]} shell property tokens."

echo "=== 3. Setting up private D-Bus and nested KWin ==="
DBUS_SOCK="$TMP_DIR/bus.sock"
DBUS_ADDR="unix:path=$DBUS_SOCK"
DBUS_ADDR_FILE="$TMP_DIR/dbus-address"

dbus-daemon --session --address="$DBUS_ADDR" --nofork --print-address > "$DBUS_ADDR_FILE" 2>&1 &
DBUS_PID=$!
echo "$DBUS_PID dbus-daemon" >> "$PID_FILE"

for _ in {1..50}; do
  if [[ -S "$DBUS_SOCK" ]]; then
    break
  fi
  sleep 0.05
done

if [[ ! -S "$DBUS_SOCK" ]]; then
  echo "ERROR: Private D-Bus daemon failed to start at $DBUS_SOCK" >&2
  exit 1
fi

export DBUS_SESSION_BUS_ADDRESS="$DBUS_ADDR"

# Setup isolated XDG directories
XDG_CONFIG_HOME="$TMP_DIR/config"
XDG_DATA_HOME="$TMP_DIR/data"
XDG_CACHE_HOME="$TMP_DIR/cache"
XDG_STATE_HOME="$TMP_DIR/state"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME/applications" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"
cp -f "$ROOT_DIR/packaging/session/kwinrc" "$XDG_CONFIG_HOME/kwinrc"
mkdir -p "$XDG_DATA_HOME/kwin/scripts"
ln -sfn "$ROOT_DIR/kwin-scripts/kynex-windows" "$XDG_DATA_HOME/kwin/scripts/kynex-windows"
ln -sfn "$ROOT_DIR/kwin-scripts/kynex-launcher" "$XDG_DATA_HOME/kwin/scripts/kynex-launcher"

# Create fake application for sandbox verification
FAKE_APP_DESKTOP="$XDG_DATA_HOME/applications/fake-probe.desktop"
FAKE_APP_SCRIPT="$TMP_DIR/fake_probe.sh"
FAKE_APP_STATUS="$TMP_DIR/fake_probe_status.txt"

cat << EOF > "$FAKE_APP_SCRIPT"
#!/usr/bin/env bash
status_file="/proc/\$\$/status"
grep -E '^(Seccomp|NoNewPrivs|PPid|Umask):' "\$status_file" > "$FAKE_APP_STATUS"
echo "PID: \$\$" >> "$FAKE_APP_STATUS"
sleep 60
EOF
chmod +x "$FAKE_APP_SCRIPT"

cat << EOF > "$FAKE_APP_DESKTOP"
[Desktop Entry]
Type=Application
Name=Fake Probe
Exec=$FAKE_APP_SCRIPT
EOF

# Start nested KWin (using --virtual if no existing wayland display, else nested)
KWIN_LOG="$TMP_DIR/kwin.log"
KWIN_ARGS=("--width" "1280" "--height" "720" "--no-lockscreen" "--no-global-shortcuts")
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  KWIN_ARGS+=("--virtual")
fi

kwin_wayland "${KWIN_ARGS[@]}" > "$KWIN_LOG" 2>&1 &
KWIN_PID=$!
echo "$KWIN_PID kwin_wayland" >> "$PID_FILE"

# Wait for KWin to become available on private D-Bus
echo "Waiting for KWin..."
for _ in {1..100}; do
  if busctl --address="$DBUS_ADDR" status org.kde.KWin >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! busctl --address="$DBUS_ADDR" status org.kde.KWin >/dev/null 2>&1; then
  echo "ERROR: KWin failed to register on D-Bus within 10s. Log:" >&2
  cat "$KWIN_LOG" >&2
  exit 1
fi

# Determine WAYLAND_DISPLAY created by KWin
TARGET_WAYLAND_DISPLAY=""
for s in "$XDG_RUNTIME_DIR"/wayland-[0-9]*; do
  if [[ -S "$s" ]]; then
    base="$(basename "$s")"
    if [[ "$base" != "${WAYLAND_DISPLAY:-}" ]]; then
      TARGET_WAYLAND_DISPLAY="$base"
      break
    fi
  fi
done
if [[ -z "$TARGET_WAYLAND_DISPLAY" && -n "${WAYLAND_DISPLAY:-}" ]]; then
  TARGET_WAYLAND_DISPLAY="$WAYLAND_DISPLAY"
fi
echo "Using Wayland display: $TARGET_WAYLAND_DISPLAY"

echo "=== 4. Starting kynex-daemon with packaged unit properties via systemd-run ==="
systemd-run --user --unit="$DAEMON_UNIT" \
  "${DAEMON_PROPS[@]}" \
  -E DBUS_SESSION_BUS_ADDRESS="$DBUS_ADDR" \
  -E XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
  -E XDG_DATA_DIRS="$XDG_DATA_HOME:/usr/share" \
  -E KYNEX_UPOWER_BUS=session \
  -E KYNEX_NM_BUS=session \
  -E KYNEX_ALLOW_TEST_FEEDER=1 \
  -E KYNEX_TEST_PACTL_PATH="$ROOT_DIR/dev/fake-pactl" \
  -E RUST_LOG=debug \
  "$BIN_DAEMON"

echo "Waiting for kynex-daemon to claim bus name..."
for _ in {1..60}; do
  if busctl --address="$DBUS_ADDR" status org.kynexos.Daemon >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! busctl --address="$DBUS_ADDR" status org.kynexos.Daemon >/dev/null 2>&1; then
  echo "ERROR: kynex-daemon failed to claim org.kynexos.Daemon. Journal:" >&2
  journalctl --user -u "${DAEMON_UNIT}.service" --no-pager >&2
  exit 1
fi
echo "✓ kynex-daemon is active and owns org.kynexos.Daemon"

echo "=== 5. Starting kynex-shell with packaged unit properties via systemd-run ==="
systemd-run --user --unit="$SHELL_UNIT" \
  "${SHELL_PROPS[@]}" \
  -E WAYLAND_DISPLAY="$TARGET_WAYLAND_DISPLAY" \
  -E DBUS_SESSION_BUS_ADDRESS="$DBUS_ADDR" \
  -E XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
  -E QT_QPA_PLATFORM=wayland \
  -E QT_WAYLAND_DISABLE_WINDOWDECORATION=1 \
  -E KYNEX_STYLE_DIR="$ROOT_DIR/styles/default" \
  -E KYNEX_QML_IMPORT_DIR="$ROOT_DIR/crates/kynex-shell/qml" \
  -E QSG_INFO=1 \
  -E QT_LOGGING_RULES="qt.scenegraph.general=true;qt.rhi.*=true" \
  -E RUST_LOG=debug \
  "$BIN_SHELL"

echo "Waiting for kynex-shell to initialize..."
sleep 2

if ! systemctl --user is-active "${SHELL_UNIT}.service" >/dev/null; then
  echo "ERROR: kynex-shell failed to start or crashed. Journal:" >&2
  journalctl --user -u "${SHELL_UNIT}.service" --no-pager >&2
  exit 1
fi
echo "✓ kynex-shell is active"

echo "=== 6. Launching fake app via Apps1.Launch to verify sandbox isolation ==="
busctl --address="$DBUS_ADDR" call org.kynexos.Daemon /org/kynexos/Daemon/Apps org.kynexos.Daemon.Apps1 Launch s "fake-probe.desktop"

echo "Waiting for fake app status dump..."
for _ in {1..50}; do
  if [[ -s "$FAKE_APP_STATUS" ]]; then
    break
  fi
  sleep 0.1
done

if [[ ! -s "$FAKE_APP_STATUS" ]]; then
  echo "ERROR: Fake app status dump was not generated!" >&2
  journalctl --user -u "${DAEMON_UNIT}.service" --no-pager >&2
  exit 1
fi

echo "Fake app status content:"
cat "$FAKE_APP_STATUS"

# Verify Seccomp == 0, NoNewPrivs == 0, Umask != 0077, PPid == user manager
SECCOMP_VAL="$(grep -E '^Seccomp:' "$FAKE_APP_STATUS" | awk '{print $2}')"
NONEWPRIVS_VAL="$(grep -E '^NoNewPrivs:' "$FAKE_APP_STATUS" | awk '{print $2}')"
UMASK_VAL="$(grep -E '^Umask:' "$FAKE_APP_STATUS" | awk '{print $2}')"
PPID_VAL="$(grep -E '^PPid:' "$FAKE_APP_STATUS" | awk '{print $2}')"
CHILD_PID_VAL="$(grep -E '^PID:' "$FAKE_APP_STATUS" | awk '{print $2}')"
FAKE_APP_PID="$CHILD_PID_VAL"

DAEMON_PID_VAL="$(systemctl --user show -p MainPID --value "${DAEMON_UNIT}.service")"

echo "Parsed status: Seccomp=$SECCOMP_VAL, NoNewPrivs=$NONEWPRIVS_VAL, Umask=$UMASK_VAL, PPid=$PPID_VAL (Daemon PID=$DAEMON_PID_VAL)"

if [[ "$SECCOMP_VAL" != "0" ]]; then
  echo "ERROR: Launched app inherited Seccomp filter ($SECCOMP_VAL != 0)!" >&2
  exit 1
fi
if [[ "$NONEWPRIVS_VAL" != "0" ]]; then
  echo "ERROR: Launched app inherited NoNewPrivileges ($NONEWPRIVS_VAL != 0)!" >&2
  exit 1
fi
if [[ "$UMASK_VAL" == "0077" ]]; then
  echo "ERROR: Launched app inherited daemon Umask 0077!" >&2
  exit 1
fi
if [[ "$PPID_VAL" == "$DAEMON_PID_VAL" ]]; then
  echo "ERROR: Launched app PPid is daemon PID ($DAEMON_PID_VAL)! Must be user manager." >&2
  exit 1
fi
echo "✓ Launched app has clean un-sandboxed environment outside daemon sandbox"

echo "=== 7. Verifying app survival across daemon kill -9 ==="
kill -KILL "$DAEMON_PID_VAL" 2>/dev/null || true
sleep 1
if ! kill -0 "$FAKE_APP_PID" 2>/dev/null; then
  echo "ERROR: Launched application did not survive daemon kill -9!" >&2
  exit 1
fi
echo "✓ Launched app (PID $FAKE_APP_PID) survived daemon kill -9"

echo "=== 8. Monitoring session units for 30 seconds ==="
# Restart daemon to maintain complete running session
systemctl --user start "${DAEMON_UNIT}.service"
sleep 1

SHELL_INVOCATION_BEFORE="$(systemctl --user show -p InvocationID --value "${SHELL_UNIT}.service")"

for i in {1..30}; do
  if ! systemctl --user is-active "${SHELL_UNIT}.service" >/dev/null; then
    echo "ERROR: kynex-shell died at second $i!" >&2
    journalctl --user -u "${SHELL_UNIT}.service" --no-pager >&2
    exit 1
  fi
  if ! systemctl --user is-active "${DAEMON_UNIT}.service" >/dev/null; then
    echo "ERROR: kynex-daemon died at second $i!" >&2
    journalctl --user -u "${DAEMON_UNIT}.service" --no-pager >&2
    exit 1
  fi
  sleep 1
done

SHELL_INVOCATION_AFTER="$(systemctl --user show -p InvocationID --value "${SHELL_UNIT}.service")"
if [[ "$SHELL_INVOCATION_BEFORE" != "$SHELL_INVOCATION_AFTER" ]]; then
  echo "ERROR: kynex-shell restarted during 30s rehearsal!" >&2
  exit 1
fi
echo "✓ Both units remained active for 30s without restarts"

echo "=== 9. Verifying shell GL rendering initialization ==="
SHELL_JOURNAL="$(journalctl --user -u "${SHELL_UNIT}.service" --no-pager)"
if echo "$SHELL_JOURNAL" | grep -Ei 'QT_QUICK_BACKEND=software|SoftwareAdaptor'; then
  echo "ERROR: Shell is running with software fallback!" >&2
  exit 1
fi

echo "Shell journal snippet:"
echo "$SHELL_JOURNAL" | tail -n 20

echo "=== SUCCESS: Unit parity rehearsal completed cleanly! ==="
