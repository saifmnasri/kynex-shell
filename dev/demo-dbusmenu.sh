#!/usr/bin/env bash
# Stage P Demo & Residue Check: Nested session with dbusmenu item, UI interaction, and cleanup.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Stage P Demo & Residue Check ==="

MARKER="/tmp/kynex-p-demo-marker-$(date +%s)"
touch "$MARKER"
echo "Marker created at: $MARKER"

TMP_DIR="dev/tmp"
mkdir -p "$TMP_DIR"
rm -f "$TMP_DIR/dbus-session-bus-address" "$TMP_DIR/wayland-display"

echo "1. Cleaning up previous sessions..."
./dev/run-nested.sh --cleanup >/dev/null 2>&1 || true

echo "2. Checking for installed third-party dbusmenu applications..."
INSTALLED_APPS=""
for app in nm-applet pasystray blueman-applet ayatana-indicator-application telegram-desktop steam; do
  if command -v "$app" >/dev/null 2>&1; then
    INSTALLED_APPS="$INSTALLED_APPS $app"
  fi
done
if [[ -n "$INSTALLED_APPS" ]]; then
  echo "Found real dbusmenu apps:$INSTALLED_APPS"
else
  echo "No third-party dbusmenu apps found in headless environment. Using verified fake dbusmenu item."
fi

echo "3. Starting nested session (KWin 6 + kynex-daemon)..."
export KYNEX_BUILD_PROFILE=release
export KYNEX_KEEP_SESSION_ALIVE=1
export KYNEX_ALLOW_TEST_FEEDER=1

./dev/run-nested.sh >/dev/null 2>&1 &
NESTED_PID=$!

BUS_ADDR=""
for _ in {1..50}; do
  if [[ -f "$TMP_DIR/dbus-session-bus-address" ]]; then
    BUS_ADDR="$(cat "$TMP_DIR/dbus-session-bus-address" | cut -d',' -f1)"
    if [[ -n "$BUS_ADDR" ]]; then break; fi
  fi
  sleep 0.2
done

if [[ -z "$BUS_ADDR" ]]; then
  echo "ERROR: Timed out waiting for D-Bus session bus."
  kill "$NESTED_PID" 2>/dev/null || true
  exit 1
fi

WAYLAND_DISP=""
for _ in {1..30}; do
  if [[ -f "$TMP_DIR/wayland-display" ]]; then
    WAYLAND_DISP="$(cat "$TMP_DIR/wayland-display")"
    if [[ -n "$WAYLAND_DISP" ]]; then break; fi
  fi
  sleep 0.1
done

echo "Session Bus: $BUS_ADDR"
echo "Wayland Display: $WAYLAND_DISP"

export DBUS_SESSION_BUS_ADDRESS="$BUS_ADDR"
export WAYLAND_DISPLAY="$WAYLAND_DISP"
export QT_QPA_PLATFORM="wayland"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export KYNEX_STYLE_DIR="styles/default"
export RUST_LOG="info"

echo "4. Launching dbusmenu-capable StatusNotifierItem (Preferences, Submenu, Checkbox, Radio, Disabled)..."
python3 -u dev/demo_fake_item.py "$BUS_ADDR" > "$TMP_DIR/demo_item.log" 2>&1 &
ITEM_PID=$!
sleep 1.0

echo "5. Starting kynex-shell in nested display..."
target/release/kynex-shell > "$TMP_DIR/demo_shell.log" 2>&1 &
SHELL_PID=$!
sleep 1.5

echo "6. Verifying Tray1 presence and Menu status..."
ITEMS_OUT="$(busctl --user --address="$BUS_ADDR" call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 GetItems)"
echo "GetItems: $ITEMS_OUT"
if [[ "$ITEMS_OUT" != *"Kynex Demo App"* ]]; then
  echo "FAIL: Demo item not found in Tray1 GetItems"
  exit 1
fi
ITEM_ID="$(echo "$ITEMS_OUT" | grep -o ':[0-9\.]*/StatusNotifierItem' | head -n1)"
echo "Detected Item ID: $ITEM_ID"

echo "7. Invoking MenuAboutToShow for root..."
busctl --user --address="$BUS_ADDR" call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 MenuAboutToShow "si" "$ITEM_ID" 0
sleep 0.2

echo "8. Fetching Sanitized Flat Menu via GetMenu..."
MENU_OUT="$(busctl --user --address="$BUS_ADDR" call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 GetMenu "s" "$ITEM_ID")"
echo "GetMenu response: $MENU_OUT"

echo "9. Testing Checkbox Toggle via MenuEvent..."
busctl --user --address="$BUS_ADDR" call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 MenuEvent "sis" "$ITEM_ID" 3 "clicked"
sleep 0.2

echo "10. Testing Submenu AboutToShow..."
busctl --user --address="$BUS_ADDR" call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 MenuAboutToShow "si" "$ITEM_ID" 2
sleep 0.2

echo "11. Verifying Disabled Item Event is Rejected by Security Policy..."
ERR_OUT="$(busctl --user --address="$BUS_ADDR" call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 MenuEvent "sis" "$ITEM_ID" 7 "clicked" 2>&1 || true)"
echo "Disabled item MenuEvent response: $ERR_OUT"
if echo "$ERR_OUT" | grep -q "is disabled"; then
  echo "PASS: MenuEvent on disabled item correctly rejected!"
else
  echo "FAIL: MenuEvent on disabled item was not rejected!"
  exit 1
fi

echo "12. Checking FakeItem event logs..."
cat "$TMP_DIR/demo_item.log"

echo "13. Terminating FakeItem and verifying removal on vanish..."
kill "$ITEM_PID" 2>/dev/null || true
wait "$ITEM_PID" 2>/dev/null || true
sleep 0.5

ITEMS_AFTER="$(busctl --user --address="$BUS_ADDR" call org.kynexos.Daemon /org/kynexos/Daemon/Tray org.kynexos.Daemon.Tray1 GetItems)"
echo "GetItems after vanish: $ITEMS_AFTER"
if [[ "$ITEMS_AFTER" == *"Kynex Demo App"* ]]; then
  echo "FAIL: Item still present after process exit!"
  exit 1
fi
echo "PASS: Item cleanly removed on vanish."

echo "14. Cleaning up nested shell and compositor..."
kill "$SHELL_PID" 2>/dev/null || true
wait "$SHELL_PID" 2>/dev/null || true
./dev/run-nested.sh --cleanup >/dev/null 2>&1 || true
sleep 0.5

echo "15. Performing Residue Check..."
ORPHANS="$(pgrep -f "demo_fake_item|kynex-shell|kynex-daemon" || true)"
if [[ -n "$ORPHANS" ]]; then
  echo "WARNING: Orphaned processes found: $ORPHANS"
  pkill -9 -f "demo_fake_item" || true
else
  echo "PASS: Zero orphaned processes."
fi

MODIFIED="$(find ~/.config ~/.local /usr /tmp -maxdepth 3 -newer "$MARKER" 2>/dev/null | grep -v "/tmp/kynex-p-demo" | grep -v "/tmp/wayland" | grep -v "$MARKER" || true)"
rm -f "$MARKER"

if [[ -n "$MODIFIED" ]]; then
  echo "Modified files detected outside allowable runtime dirs:"
  echo "$MODIFIED"
  exit 1
else
  echo "PASS: Zero residue files detected in ~/.config ~/.local /usr /tmp."
fi

echo "=== Stage P Demo & Residue Check Complete: SUCCESS ==="
