#!/usr/bin/env bash
# collect-logs.sh: Gathers diagnostic session logs for Kynex Shell.
# Collects current-boot journal slices, logind status, unit status, filtered env,
# DumpState, introspects, last 200 lines of app logs, versions, and caps archive at 20 MB.
# Part of Kynex Shell. Public-safe, no hardcoded private paths.
set -euo pipefail

TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
ARCHIVE="${HOME}/kynex-session-logs-${TIMESTAMP}.tar.gz"
MAX_ARCHIVE_BYTES=$((20 * 1024 * 1024)) # 20 MB cap

TMP_DIR="$(mktemp -d -t kynex-logs-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

echo "=== Collecting Kynex Session Diagnostic Logs ==="
echo "Staging directory: $TMP_DIR"
echo "Current boot ID: $(cat /proc/sys/kernel/random/boot_id 2>/dev/null || echo 'unknown')"
echo "Target archive: $ARCHIVE (cap: 20 MB)"

sanitize_file() {
  local target="$1"
  if [[ -f "$target" ]]; then
    # Redact sensitive values and note home directory
    sed -i -E \
      -e 's/(TOKEN|PASSWORD|SECRET|KEY|AUTH|CREDENTIAL|COOKIE|PASSWD|PIN)=[^\t\n\r "]+/\1=[REDACTED]/gi' \
      -e "s|${HOME}|~|g" \
      "$target" || true
  fi
}

# 1. User Journal for current boot only
echo "-> Collecting current-boot user journal..."
journalctl --user -b 0 --no-pager > "$TMP_DIR/journal-user.log" 2>&1 || true
sanitize_file "$TMP_DIR/journal-user.log"

# 2. System Display Manager and Compositor Journal for current boot only
echo "-> Collecting current-boot system DM and KWin journal..."
journalctl -b 0 \
  _COMM=kwin_wayland \
  _COMM=kwin_wayland_wrapper \
  _COMM=gdm3 \
  _COMM=gdm-wayland-session \
  _COMM=gdm-session-worker \
  _SYSTEMD_UNIT=gdm.service \
  _SYSTEMD_UNIT=gdm3.service \
  _SYSTEMD_UNIT=display-manager.service \
  --no-pager > "$TMP_DIR/journal-compositor-dm.log" 2>&1 || true
sanitize_file "$TMP_DIR/journal-compositor-dm.log"

# 3. Systemd User Units Status and Environment
echo "-> Collecting systemd unit states and environment..."
{
  echo "=== systemctl --user show -p Environment ==="
  systemctl --user show -p Environment 2>&1 || true
  echo ""
  echo "=== kynex-session.target ==="
  systemctl --user status kynex-session.target --no-pager || true
  echo ""
  echo "=== kynex-daemon.service ==="
  systemctl --user status kynex-daemon.service --no-pager || true
  echo ""
  echo "=== kynex-shell.service ==="
  systemctl --user status kynex-shell.service --no-pager || true
  echo ""
  echo "=== Failed user units ==="
  systemctl --user --failed --no-pager || true
} > "$TMP_DIR/systemd-user-units.log" 2>&1
sanitize_file "$TMP_DIR/systemd-user-units.log"

# 4. D-Bus Introspect and Shell Debug DumpState
echo "-> Collecting D-Bus introspect and DumpState..."
{
  echo "=== Shell DumpState ==="
  busctl --user call org.kynexos.Shell /org/kynexos/Shell org.kynexos.Shell.Debug1 DumpState 2>&1 || echo "Shell Debug1 DumpState not available"
  echo ""
  echo "=== Shell ModelsReady Property ==="
  busctl --user get-property org.kynexos.Shell /org/kynexos/Shell org.kynexos.Shell.Debug1 ModelsReady 2>&1 || true
  echo ""
  echo "=== Shell Introspect ==="
  busctl --user introspect org.kynexos.Shell /org/kynexos/Shell 2>&1 || echo "Shell not active on user bus"
  echo ""
  echo "=== Daemon Introspect ==="
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon 2>&1 || echo "Daemon not active on user bus"
  echo ""
  echo "=== Daemon Windows1 Introspect ==="
  busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon/Windows org.kynexos.Daemon.Windows1 2>&1 || true
  echo ""
  echo "=== kglobalaccel status ==="
  busctl --user status org.kde.kglobalaccel 2>&1 || echo "kglobalaccel not active on user bus"
} > "$TMP_DIR/dbus-debug-dump.log" 2>&1
sanitize_file "$TMP_DIR/dbus-debug-dump.log"

# 5. KWin Script State
echo "-> Collecting KWin script state..."
{
  echo "=== KWin Scripting Status ==="
  busctl --user introspect org.kde.KWin /Scripting org.kde.kwin.Scripting 2>&1 || true
  echo ""
  echo "=== Loaded Scripts (via busctl --user call) ==="
  busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting isScriptLoaded s "kynex-windows" 2>&1 || true
  busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting isScriptLoaded s "kynex-launcher" 2>&1 || true
} > "$TMP_DIR/kwin-scripts.log" 2>&1

# 6. Logind Sessions and Seats
echo "-> Collecting logind sessions and seats..."
{
  echo "=== loginctl list-sessions ==="
  loginctl list-sessions --no-pager || true
  echo ""
  echo "=== loginctl list-seats ==="
  loginctl list-seats --no-pager || true
  echo ""
  echo "=== Current Session Status ==="
  loginctl session-status --no-pager || true
  echo ""
  echo "=== Detailed Session Properties ==="
  while read -r sid _rest || [[ -n "$sid" ]]; do
    [[ -z "$sid" || "$sid" == "SESSION" ]] && continue
    echo "--- Session $sid ---"
    loginctl show-session "$sid" --no-pager || true
  done < <(loginctl list-sessions --no-legend 2>/dev/null || true)
} > "$TMP_DIR/logind-status.log" 2>&1
sanitize_file "$TMP_DIR/logind-status.log"

# 7. Environment Dump (Secrets and sensitive tokens redacted)
echo "-> Collecting filtered environment..."
env | grep -vEi '(KEY|TOKEN|SECRET|PASS|AUTH|CREDENTIAL|COOKIE|SIGNATURE|PRIVATE)' | sort > "$TMP_DIR/environment-filtered.log"
sanitize_file "$TMP_DIR/environment-filtered.log"

# 8. Kernel Dmesg Tail (last 300 lines)
echo "-> Collecting dmesg tail..."
dmesg | tail -n 300 > "$TMP_DIR/dmesg-tail.log" 2>&1 || true
sanitize_file "$TMP_DIR/dmesg-tail.log"

# 9. Application Logs (last 200 lines each, NEVER whole dev/logs)
echo "-> Collecting application logs (last 200 lines each)..."
mkdir -p "$TMP_DIR/app-logs"
if [[ -d "dev/logs" ]]; then
  for f in dev/logs/*.log; do
    if [[ -f "$f" ]]; then
      b="$(basename "$f")"
      tail -n 200 "$f" > "$TMP_DIR/app-logs/${b}" 2>/dev/null || true
      sanitize_file "$TMP_DIR/app-logs/${b}"
    fi
  done
fi
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/kynex"
if [[ -d "$STATE_DIR" ]]; then
  for f in "$STATE_DIR"/*.log; do
    if [[ -f "$f" ]]; then
      b="$(basename "$f")"
      tail -n 200 "$f" > "$TMP_DIR/app-logs/${b}" 2>/dev/null || true
      sanitize_file "$TMP_DIR/app-logs/${b}"
    fi
  done
fi

# 10. Platform and Package Versions
echo "-> Collecting platform versions..."
{
  echo "=== Kernel ==="
  uname -a
  echo ""
  echo "=== Compositor ==="
  kwin_wayland --version 2>&1 || true
  echo ""
  echo "=== System Packages ==="
  dpkg -l gdm3 kwin-wayland systemd 2>/dev/null || true
  echo ""
  echo "=== systemd ==="
  systemd --version 2>&1 || true
  echo ""
  echo "=== Rust & Compiler ==="
  rustc --version 2>&1 || true
} > "$TMP_DIR/versions.log" 2>&1
sanitize_file "$TMP_DIR/versions.log"

# 11. Pack Archive & Check 20 MB Cap
echo "-> Creating archive $ARCHIVE..."
tar -czf "$ARCHIVE" -C "$TMP_DIR" .

ARCHIVE_SIZE=$(stat -c%s "$ARCHIVE" 2>/dev/null || stat -f%z "$ARCHIVE" 2>/dev/null || echo 0)
if [[ "$ARCHIVE_SIZE" -gt "$MAX_ARCHIVE_BYTES" ]]; then
  echo "Warning: Archive size ($ARCHIVE_SIZE bytes) exceeded 20 MB cap. Truncating journal logs..."
  head -n 2000 "$TMP_DIR/journal-user.log" > "$TMP_DIR/journal-user.log.trunc" && mv "$TMP_DIR/journal-user.log.trunc" "$TMP_DIR/journal-user.log"
  head -n 2000 "$TMP_DIR/journal-compositor-dm.log" > "$TMP_DIR/journal-compositor-dm.log.trunc" && mv "$TMP_DIR/journal-compositor-dm.log.trunc" "$TMP_DIR/journal-compositor-dm.log"
  tar -czf "$ARCHIVE" -C "$TMP_DIR" .
  ARCHIVE_SIZE=$(stat -c%s "$ARCHIVE" 2>/dev/null || echo 0)
fi

echo "=== Log Collection Complete ==="
echo "Archive: $ARCHIVE ($(du -h "$ARCHIVE" | awk '{print $1}')) [Bytes: $ARCHIVE_SIZE / Max: $MAX_ARCHIVE_BYTES]"
echo "Contents:"
tar -ztvf "$ARCHIVE"
