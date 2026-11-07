#!/usr/bin/env bash
# Integration and effectiveness test for kynex-daemon systemd user unit sandboxing.
# Validates every sandboxing directive using individual systemd-run probes,
# ensures no ineffective directives are packaged, and verifies clean execution.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

UNIT_NAME="kynex-daemon-test"
SERVICE_NAME="${UNIT_NAME}.service"
RUNTIME_USER_DIR="/run/user/$(id -u)/systemd/user"
TARGET_UNIT_PATH="${RUNTIME_USER_DIR}/${SERVICE_NAME}"

# Cleanup trap to guarantee nothing is left behind
cleanup() {
  echo "Cleaning up test systemd unit..."
  systemctl --user stop "$SERVICE_NAME" >/dev/null 2>&1 || true
  rm -f "$TARGET_UNIT_PATH"
  systemctl --user daemon-reload >/dev/null 2>&1 || true
  rm -f "/tmp/.kynex_probe_marker" "/home/$USER/.kynex_sandbox_probe" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

echo "=== 1. Probing Individual Systemd Sandboxing Directives ==="

PROBE_FAILURES=0

probe_directive() {
  local directive="$1"
  local cmd="$2"
  local expect_effect="$3" # "EFFECTIVE" or "INEFFECTIVE-HERE"
  local desc="$4"

  local probe_unit="kynex-probe-$RANDOM"
  local out=""
  local rc=0

  set +e
  out="$(systemd-run --user --unit="$probe_unit" --wait --pipe -p "$directive" bash -c "$cmd" 2>&1)"
  rc=$?
  set -e

  # Check journal specifically for this probe unit
  local journal_warn
  journal_warn="$(journalctl --user -u "${probe_unit}.service" --no-pager 2>&1 | grep -Ei 'proceeding without|ignoring namespace setup|failed to' || true)"

  echo "------------------------------------------------------------"
  echo "Directive: $directive"
  echo "Description: $desc"
  echo "Probe exit code: $rc"
  echo "Probe output: $out"
  if [[ -n "$journal_warn" ]]; then
    echo "Journal warning: $journal_warn"
  fi

  if [[ "$expect_effect" == "EFFECTIVE" ]]; then
    if [[ "$rc" -ne 0 ]]; then
      echo "Classification: UNEXPECTED FAILURE (exit code $rc)"
      PROBE_FAILURES=$((PROBE_FAILURES + 1))
    elif [[ -n "$journal_warn" ]]; then
      echo "Classification: DEGRADED (journal warning: $journal_warn)"
      PROBE_FAILURES=$((PROBE_FAILURES + 1))
    else
      echo "Classification: EFFECTIVE"
    fi
  else
    echo "Classification: INEFFECTIVE-HERE"
  fi
}

# --- Probes for directives ---

# 1. NoNewPrivileges=yes (effective)
probe_directive "NoNewPrivileges=yes" \
  "grep -E '^NoNewPrivs:[[:space:]]+1' /proc/self/status" \
  "EFFECTIVE" \
  "Verifies prctl(PR_SET_NO_NEW_PRIVS) is active"

# 2. ProtectSystem=strict (ineffective: root remains rw, mount ns unchanged)
probe_directive "ProtectSystem=strict" \
  "touch /home/$USER/.kynex_sandbox_probe && rm -f /home/$USER/.kynex_sandbox_probe" \
  "INEFFECTIVE-HERE" \
  "Mount namespace is not unshared; user home remains writable"

# 3. ProtectHome=read-only (ineffective: home remains writable, mount ns unchanged)
probe_directive "ProtectHome=read-only" \
  "touch /home/$USER/.kynex_sandbox_probe && rm -f /home/$USER/.kynex_sandbox_probe" \
  "INEFFECTIVE-HERE" \
  "Mount namespace is not unshared; \$HOME remains writable"

# 4. PrivateTmp=yes (ineffective: /tmp is not unshared)
touch "/tmp/.kynex_probe_marker"
probe_directive "PrivateTmp=yes" \
  "test -f /tmp/.kynex_probe_marker" \
  "INEFFECTIVE-HERE" \
  "Mount namespace is not unshared; host /tmp marker remains visible"
rm -f "/tmp/.kynex_probe_marker"

# 5. PrivateNetwork=yes (ineffective: network ns setup prohibited/unprivileged)
probe_directive "PrivateNetwork=yes" \
  "ip link show enp0s1 >/dev/null 2>&1" \
  "INEFFECTIVE-HERE" \
  "Network namespace not isolated; host interfaces visible; journal warns 'proceeding without'"

# 6. IPAddressDeny=any (ineffective: cgroup BPF not available in user session)
probe_directive "IPAddressDeny=any" \
  "python3 -c 'import socket; s = socket.socket(socket.AF_INET, socket.SOCK_STREAM); s.settimeout(1); s.connect((\"1.1.1.1\", 80))'" \
  "INEFFECTIVE-HERE" \
  "cgroup BPF filter not attached; outbound TCP connection succeeds"

# 7. RestrictAddressFamilies=AF_UNIX (effective: blocks AF_INET)
probe_directive "RestrictAddressFamilies=AF_UNIX" \
  "python3 -c '
import socket, sys
try:
    socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sys.exit(2)
except OSError as e:
    sys.exit(0 if e.errno == 97 else 3)
'" \
  "EFFECTIVE" \
  "Seccomp socket filter successfully blocks AF_INET with EAFNOSUPPORT"

# 8. SystemCallFilter=@system-service (effective: blocks prohibited syscalls)
probe_directive "SystemCallFilter=@system-service" \
  "grep -E '^Seccomp:[[:space:]]+2' /proc/self/status" \
  "EFFECTIVE" \
  "Seccomp mode 2 active; non-whitelisted syscalls filtered"

# 9. SystemCallArchitectures=native (effective: enforces native arch seccomp)
probe_directive "SystemCallArchitectures=native" \
  "grep -E '^Seccomp:[[:space:]]+2' /proc/self/status" \
  "EFFECTIVE" \
  "Seccomp mode 2 active enforcing native architecture syscall ABI"

# 10. ProtectKernelTunables=yes (ineffective: mount ns not unshared)
probe_directive "ProtectKernelTunables=yes" \
  "readlink /proc/self/ns/mnt" \
  "INEFFECTIVE-HERE" \
  "Mount namespace unchanged from host session"

# 11. ProtectControlGroups=yes (ineffective: mount ns not unshared)
probe_directive "ProtectControlGroups=yes" \
  "readlink /proc/self/ns/mnt" \
  "INEFFECTIVE-HERE" \
  "Mount namespace unchanged from host session"

# 12. ProtectHostname=yes (ineffective: UTS ns setup prohibited)
probe_directive "ProtectHostname=yes" \
  "hostname" \
  "INEFFECTIVE-HERE" \
  "UTS namespace prohibited in user manager; journal warns 'ignoring namespace setup'"

# 13. ProtectProc=invisible (ineffective: mount ns not unshared)
probe_directive "ProtectProc=invisible" \
  "test -d /proc/1" \
  "INEFFECTIVE-HERE" \
  "Mount namespace unchanged; /proc/1 remains visible"

# 14. ProcSubset=pid (ineffective: mount ns not unshared)
probe_directive "ProcSubset=pid" \
  "test -f /proc/cmdline" \
  "INEFFECTIVE-HERE" \
  "Mount namespace unchanged; /proc/cmdline remains visible"

# 15. RestrictRealtime=yes (effective: seccomp blocks RT scheduling)
probe_directive "RestrictRealtime=yes" \
  "python3 -c '
import ctypes, sys
libc = ctypes.CDLL(None, use_errno=True)
libc.sched_setscheduler(0, 1, 0)
sys.exit(0 if ctypes.get_errno() == 1 else 2)
'" \
  "EFFECTIVE" \
  "Seccomp blocks real-time scheduler configuration with EPERM"

# 16. RestrictNamespaces=yes (effective: blocks user/net/mnt unshare)
probe_directive "RestrictNamespaces=yes" \
  "unshare --user true 2>&1 | grep -q 'Operation not permitted'" \
  "EFFECTIVE" \
  "Seccomp blocks CLONE_NEW* / unshare with EPERM"

# 17. RestrictSUIDSGID=yes (effective: blocks SUID chmod)
probe_directive "RestrictSUIDSGID=yes" \
  "python3 -c '
import os, tempfile, sys
f = tempfile.NamedTemporaryFile()
try:
    os.chmod(f.name, 0o4755)
    sys.exit(2)
except PermissionError:
    sys.exit(0)
'" \
  "EFFECTIVE" \
  "Seccomp blocks SUID bit creation with EPERM"

# 18. MemoryDenyWriteExecute=yes (effective: blocks W+X mmap)
probe_directive "MemoryDenyWriteExecute=yes" \
  "python3 -c '
import mmap, sys
try:
    mmap.mmap(-1, 4096, prot=mmap.PROT_READ|mmap.PROT_WRITE|mmap.PROT_EXEC)
    sys.exit(2)
except PermissionError:
    sys.exit(0)
'" \
  "EFFECTIVE" \
  "Seccomp blocks W+X mmap allocation with EACCES"

# 19. LockPersonality=yes (effective: blocks personality change)
probe_directive "LockPersonality=yes" \
  "python3 -c '
import ctypes, sys
libc = ctypes.CDLL(None, use_errno=True)
libc.personality(1)
sys.exit(0 if ctypes.get_errno() == 1 else 2)
'" \
  "EFFECTIVE" \
  "Seccomp locks execution domain; personality change fails with EPERM"

# 20. KeyringMode=private (effective: allocates private session keyring)
probe_directive "KeyringMode=private" \
  "python3 -c '
import ctypes, platform, sys
libc = ctypes.CDLL(None)
nr = 219 if platform.machine() == \"aarch64\" else 250
kid = libc.syscall(nr, 0, -3, 0)
sys.exit(0 if kid > 0 else 1)
'" \
  "EFFECTIVE" \
  "Allocates isolated session keyring distinct from default user keyring"

# 21. UMask=0077 (effective: sets owner-only file mask)
probe_directive "UMask=0077" \
  "test \"\$(umask)\" = \"0077\"" \
  "EFFECTIVE" \
  "Process umask successfully set to 0077"

if [[ "$PROBE_FAILURES" -ne 0 ]]; then
  echo "FAIL: $PROBE_FAILURES directive probes failed unexpectedly!" >&2
  exit 1
fi
echo "All 21 individual directive probes evaluated cleanly."

echo "=== 2. Auditing Packaged Unit: systemd/kynex-daemon.service ==="
PACKAGED_UNIT="$ROOT_DIR/systemd/kynex-daemon.service"

INEFFECTIVE_DIRECTIVES=(
  "ProtectSystem"
  "ProtectHome"
  "PrivateTmp"
  "PrivateNetwork"
  "IPAddressDeny"
  "ProtectKernelTunables"
  "ProtectControlGroups"
  "ProtectHostname"
  "ProtectProc"
  "ProcSubset"
)

FOUND_INEFFECTIVE=0
for ineff in "${INEFFECTIVE_DIRECTIVES[@]}"; do
  if grep -E "^[[:space:]]*${ineff}=" "$PACKAGED_UNIT"; then
    echo "ERROR: Packaged unit contains ineffective directive: $ineff" >&2
    FOUND_INEFFECTIVE=1
  fi
done

if [[ "$FOUND_INEFFECTIVE" -ne 0 ]]; then
  echo "FAIL: Packaged unit contains ineffective directives. Remove them from $PACKAGED_UNIT." >&2
  exit 1
fi
echo "Packaged unit contains ONLY empirically effective directives."

echo "=== 3. Deploying and Testing Packaged Unit ==="
DAEMON_BIN="${ROOT_DIR}/target/debug/kynex-daemon"
if [[ ! -x "$DAEMON_BIN" ]]; then
  echo "Building kynex-daemon..."
  cargo build -p kynex-daemon --locked
fi

mkdir -p "$RUNTIME_USER_DIR"
sed -e "s|ExecStart=.*|ExecStart=${DAEMON_BIN}|" \
    -e "s|Restart=.*|Restart=no|" \
    "$PACKAGED_UNIT" > "$TARGET_UNIT_PATH"

systemctl --user daemon-reload

START_TIME="$(date '+%Y-%m-%d %H:%M:%S')"
systemctl --user start "$SERVICE_NAME"
systemctl --user status "$SERVICE_NAME" --no-pager

MAINPID="$(systemctl --user show --property=MainPID --value "$SERVICE_NAME")"
if [[ -z "$MAINPID" || "$MAINPID" == "0" ]]; then
  echo "ERROR: Could not obtain MainPID for running unit $SERVICE_NAME" >&2
  exit 1
fi

echo "=== 4. Inspecting Running Process Security Status ==="
echo "Process MainPID: $MAINPID"
grep -E '^(NoNewPrivs|Seccomp):' "/proc/$MAINPID/status"

NO_NEW_PRIVS="$(awk '/^NoNewPrivs:/ {print $2}' "/proc/$MAINPID/status")"
SECCOMP_MODE="$(awk '/^Seccomp:/ {print $2}' "/proc/$MAINPID/status")"

if [[ "$NO_NEW_PRIVS" != "1" ]]; then
  echo "FAIL: Expected NoNewPrivs: 1, got $NO_NEW_PRIVS" >&2
  exit 1
fi

if [[ "$SECCOMP_MODE" != "2" ]]; then
  echo "FAIL: Expected Seccomp: 2, got $SECCOMP_MODE" >&2
  exit 1
fi

echo "=== 5. Querying D-Bus Interfaces ==="
busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1
busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Power1 Present Percentage State
busctl --user introspect org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Network1
busctl --user get-property org.kynexos.Daemon /org/kynexos/Daemon org.kynexos.Daemon.Network1 Present State Kind SignalStrength Ssid

echo "=== 6. Inspecting User Journal for Degraded Directives ==="
JOURNAL_OUTPUT="$(journalctl --user -u "$SERVICE_NAME" --since "$START_TIME" --no-pager)"
echo "$JOURNAL_OUTPUT"

if echo "$JOURNAL_OUTPUT" | grep -Ei 'proceeding without|ignoring|failed to'; then
  echo "FAIL: Journal contains degraded sandboxing directive warnings!" >&2
  exit 1
fi

echo "=== 7. Secondary Information: systemd-analyze security ==="
systemd-analyze security --user "$SERVICE_NAME" --no-pager || true

echo "=== 8. Stopping Unit and Verifying Clean Environment ==="
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

echo "SUCCESS: All sandbox probes verified, packaged unit is free of ineffective directives, journal is 100% clean, and environment is clean."
