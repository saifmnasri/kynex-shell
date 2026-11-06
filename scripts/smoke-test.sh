#!/usr/bin/env bash
# Headless smoke test: runs kynex-shell in kwin_wayland --virtual to verify startup without QML errors.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

ROOT_DIR="$(pwd)"
LOG_DIR="$ROOT_DIR/dev/logs"
mkdir -p "$LOG_DIR"

echo "Building kynex-shell..."
cargo build -p kynex-shell

TMP_DIR="$(mktemp -d)"
cleanup() {
  trap - EXIT INT TERM
  pkill -TERM -P $$ 2>/dev/null || true
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

SESSION_SH="$TMP_DIR/session.sh"
cat << 'EOF' > "$SESSION_SH"
#!/usr/bin/env bash
set -euo pipefail
# Run kynex-shell for 3 seconds, then exit successfully
timeout 3s "$SHELL_BIN" || {
  rc=$?
  # Exit code 124 is standard timeout success for a continuous daemon/shell
  if [[ $rc -eq 124 || $rc -eq 0 ]]; then
    exit 0
  else
    exit $rc
  fi
}
EOF
chmod +x "$SESSION_SH"

SMOKE_LOG="$LOG_DIR/smoke_test.log"

echo "Running headless smoke test with kwin_wayland --virtual..."
export SHELL_BIN="$ROOT_DIR/target/debug/kynex-shell"

dbus-run-session bash -c "
  export XDG_RUNTIME_DIR=\"$TMP_DIR/xdg\"
  export XDG_CONFIG_HOME=\"$TMP_DIR/config\"
  export XDG_DATA_HOME=\"$TMP_DIR/data\"
  export XDG_CACHE_HOME=\"$TMP_DIR/cache\"
  mkdir -p \"\$XDG_RUNTIME_DIR\" \"\$XDG_CONFIG_HOME\" \"\$XDG_DATA_HOME\" \"\$XDG_CACHE_HOME\"
  chmod 700 \"\$XDG_RUNTIME_DIR\"

  export QT_QPA_PLATFORM=wayland
  export KYNEX_STYLE_DIR=\"$ROOT_DIR/styles/default\"

  kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --exit-with-session \"$SESSION_SH\" > \"$SMOKE_LOG\" 2>&1
"

echo "Smoke test log output:"
cat "$SMOKE_LOG"

# Check log for any QML errors or crashes
if grep -iE "TypeError|qml:.*error|ReferenceError" "$SMOKE_LOG"; then
  echo "FAIL: Detected QML errors in smoke test log"
  exit 1
fi

echo "Smoke test PASSED: kynex-shell started and ran in virtual KWin without errors."

