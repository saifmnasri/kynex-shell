#!/usr/bin/env python3
"""
Taskbar QML Mutants Verification (Stage R4.3).

Tests the three named mutants:
1. MUTANT-NO-NOTIFY (KYNEX_TEST_MUTANT_NO_NOTIFY=1):
   Suppresses Qt model reset/countChanged signals (the original bug condition).
   Must fail on signal spy or delegate count assertion.
2. MUTANT-ZERO-WIDTH (KYNEX_TEST_MUTANT_ZERO_WIDTH=1):
   Forces taskbar button width to 0.
   Must fail on delegate dimension assertion (width > 0).
3. MUTANT-HIDDEN-DELEGATE (KYNEX_TEST_MUTANT_HIDDEN_DELEGATE=1):
   Forces taskbar button visibility to false.
   Must fail on delegate isVisible assertion.

All three mutants must be KILLED by the harness.
"""

import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parent.parent.parent

DBUS_CONFIG_TEMPLATE = """<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={socket_path}</listen>
  <auth>EXTERNAL</auth>
  <servicedir>{empty_services}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"""


def stage_destdir() -> str:
    destdir = tempfile.mkdtemp(prefix="kynex-destdir-mutants-")
    cmd = [str(ROOT_DIR / "packaging/session/install-session.sh"), "--force"]
    env = os.environ.copy()
    env["DESTDIR"] = destdir
    res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR), capture_output=True, text=True)
    if res.returncode != 0:
        print(f"ERROR: install-session.sh failed:\n{res.stderr}", file=sys.stderr)
        shutil.rmtree(destdir, ignore_errors=True)
        sys.exit(1)
    return destdir


def run_single_mutant(mutant_name: str, env_var: str, destdir: str) -> bool:
    print(f"\n==================================================================")
    print(f"[MUTANT TEST] Testing {mutant_name} ({env_var}=1)")
    print(f"==================================================================")

    tmp_dir = tempfile.mkdtemp(prefix=f"kynex-mutant-{mutant_name}-")
    socket_path = os.path.join(tmp_dir, "dbus.sock")
    bus_address = f"unix:path={socket_path}"

    empty_services = os.path.join(tmp_dir, "empty-services")
    os.makedirs(empty_services, exist_ok=True)
    conf_path = os.path.join(tmp_dir, "session.conf")
    with open(conf_path, "w", encoding="utf-8") as f:
        f.write(DBUS_CONFIG_TEMPLATE.format(socket_path=socket_path, empty_services=empty_services))

    dbus_proc = subprocess.Popen(["dbus-daemon", f"--config-file={conf_path}", "--nofork"])
    time.sleep(0.15)

    env = os.environ.copy()
    env["DBUS_SESSION_BUS_ADDRESS"] = bus_address
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["QT_QUICK_BACKEND"] = "software"
    env["KYNEX_STYLE_DIR"] = f"{destdir}/usr/share/kynex/styles/default"
    env["KYNEX_QML_IMPORT_DIR"] = f"{destdir}/usr/share/kynex/qml"
    env["KYNEX_ALLOW_TEST_FEEDER"] = "1"
    env["RUST_LOG"] = "kynex_shell=info,kynex_daemon=info"
    env["XDG_DATA_DIRS"] = f"{destdir}/usr/share:/usr/share"
    env[env_var] = "1"

    daemon_bin = f"{destdir}/usr/local/bin/kynex-daemon"
    daemon_proc = subprocess.Popen([daemon_bin], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(0.3)

    harness_bin = str(ROOT_DIR / "target/debug/qml_taskbar_harness")
    cmd = [harness_bin, "--destdir", destdir, "--geometry", "1920x1080@1"]
    res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR), capture_output=True, text=True)

    daemon_proc.terminate()
    try:
        daemon_proc.wait(timeout=1.0)
    except subprocess.TimeoutExpired:
        daemon_proc.kill()

    dbus_proc.terminate()
    try:
        dbus_proc.wait(timeout=1.0)
    except subprocess.TimeoutExpired:
        dbus_proc.kill()

    shutil.rmtree(tmp_dir, ignore_errors=True)

    print(f"[MUTANT TEST] Exit code: {res.returncode}")
    print(f"[MUTANT TEST] Harness output:\n{res.stdout}\n{res.stderr}")

    if res.returncode != 0:
        print(f"[MUTANT TEST] Result: {mutant_name} was KILLED as expected!")
        return True
    else:
        print(f"[MUTANT TEST] Result: ERROR: {mutant_name} SURVIVED! Expected failure.")
        return False


def main():
    destdir = stage_destdir()
    mutants = [
        ("MUTANT-NO-NOTIFY", "KYNEX_TEST_MUTANT_NO_NOTIFY"),
        ("MUTANT-ZERO-WIDTH", "KYNEX_TEST_MUTANT_ZERO_WIDTH"),
        ("MUTANT-HIDDEN-DELEGATE", "KYNEX_TEST_MUTANT_HIDDEN_DELEGATE"),
    ]

    all_killed = True
    for name, env_var in mutants:
        killed = run_single_mutant(name, env_var, destdir)
        if not killed:
            all_killed = False

    shutil.rmtree(destdir, ignore_errors=True)

    if all_killed:
        print("\n==================================================================")
        print("[MUTANT TEST SUMMARY] ALL 3 NAMED MUTANTS WERE KILLED AS EXPECTED!")
        print("==================================================================")
        sys.exit(0)
    else:
        print("\n==================================================================")
        print("[MUTANT TEST SUMMARY] ONE OR MORE MUTANTS SURVIVED!")
        print("==================================================================")
        sys.exit(1)


if __name__ == "__main__":
    main()
