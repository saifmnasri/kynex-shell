#!/usr/bin/env bash
# collect-logs.sh: Gathers diagnostic session logs for Kynex Shell.
# Collects journalctl, logind status, unit status, filtered env, dmesg, app logs, and versions.
# Writes to ~/kynex-session-logs-<timestamp>.tar.gz.
# Part of Kynex Shell. Public-safe, no hardcoded private paths.
set -euo pipefail

TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
ARCHIVE="${HOME}/kynex-session-logs-${TIMESTAMP}.tar.gz"

TMP_DIR="$(mktemp -d -t kynex-logs-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

echo "=== Collecting Kynex Session Diagnostic Logs ==="
echo "Staging directory: $TMP_DIR"

# 1. User Journal
echo "-> Collecting user journal..."
journalctl --user -b --no-pager > "$TMP_DIR/journal-user.log" 2>&1 || true

# 2. System Display Manager and Compositor Journal
echo "-> Collecting system DM and KWin journal..."
journalctl -b \
  _COMM=kwin_wayland \
  _COMM=kwin_wayland_wrapper \
  _COMM=gdm3 \
  _COMM=gdm-wayland-session \
  _COMM=gdm-session-worker \
  _SYSTEMD_UNIT=gdm.service \
  _SYSTEMD_UNIT=gdm3.service \
  _SYSTEMD_UNIT=display-manager.service \
  --no-pager > "$TMP_DIR/journal-compositor-dm.log" 2>&1 || true

# 3. Systemd User Units Status
echo "-> Collecting systemd unit states..."
{
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

# 3b. Global Shortcuts Subsystem (kglobalacceld)
echo "-> Collecting kglobalacceld status and bus registration..."
{
  echo "=== busctl --user list | grep -i kglobalaccel ==="
  busctl --user list 2>/dev/null | grep -i kglobalaccel || echo "kglobalaccel not active on user bus"
  echo ""
  echo "=== plasma-kglobalaccel.service status ==="
  systemctl --user status plasma-kglobalaccel.service --no-pager || true
} > "$TMP_DIR/kglobalaccel-status.log" 2>&1

# 4. Logind Sessions and Seats
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

# 5. Environment Dump (Secrets and sensitive tokens redacted)
echo "-> Collecting filtered environment..."
env | grep -vEi '(KEY|TOKEN|SECRET|PASS|AUTH|CREDENTIAL|COOKIE|SIGNATURE)' | sort > "$TMP_DIR/environment-filtered.log"

# 6. Kernel Dmesg Tail
echo "-> Collecting dmesg tail..."
dmesg | tail -n 300 > "$TMP_DIR/dmesg-tail.log" 2>&1 || true

# 7. Application Logs (dev/logs or ~/.local/state/kynex)
echo "-> Collecting application logs..."
mkdir -p "$TMP_DIR/app-logs"
if [[ -d "dev/logs" ]]; then
  cp -rf dev/logs/* "$TMP_DIR/app-logs/" 2>/dev/null || true
fi
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/kynex"
if [[ -d "$STATE_DIR" ]]; then
  cp -rf "$STATE_DIR"/* "$TMP_DIR/app-logs/" 2>/dev/null || true
fi

# 8. Platform and Package Versions
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

# 9. Pack Archive
echo "-> Creating archive $ARCHIVE..."
tar -czf "$ARCHIVE" -C "$TMP_DIR" .

echo "=== Log Collection Complete ==="
echo "Archive: $ARCHIVE ($(du -h "$ARCHIVE" | awk '{print $1}'))"
echo "Contents:"
tar -ztvf "$ARCHIVE"
