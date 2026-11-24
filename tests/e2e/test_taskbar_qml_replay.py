#!/usr/bin/env python3
"""
QML Taskbar Replay Test (Stage R4.1).

Instantiates the REAL styles/default/Shell.qml from an installed-layout temp DESTDIR
with the REAL cxx-qt taskbar/window models (not mocks), under QT_QPA_PLATFORM=offscreen
and QT_QUICK_BACKEND=software, at three geometries:
- 1920x1080 DPR 1
- 2520x1400 DPR 2
- 3840x2160 DPR 2

Replays the real captured sequence (Calculator, ActiveChanged, Minimized=true, Terminal)
and verifies at each step that:
1. Qt model emitted rowsInserted/dataChanged/modelReset (QSignalSpy)
2. ListView count equals entries
3. Every delegate is visible with opacity > 0 and width/height > 0
4. Lies inside the panel
5. Does not overlap clock or status area
6. Shows non-empty label or badge.
"""

import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import dbus
import dbus.bus

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
    """Installs session files into a temporary DESTDIR staging root."""
    destdir = tempfile.mkdtemp(prefix="kynex-destdir-")
    print(f"[QML E2E] Staging install-session into {destdir}...")
    cmd = [str(ROOT_DIR / "packaging/session/install-session.sh"), "--force"]
    env = os.environ.copy()
    env["DESTDIR"] = destdir
    res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR), capture_output=True, text=True)
    if res.returncode != 0:
        print(f"ERROR: install-session.sh failed:\n{res.stderr}", file=sys.stderr)
        shutil.rmtree(destdir, ignore_errors=True)
        sys.exit(1)

    assert os.path.isdir(f"{destdir}/usr/share/kynex/styles/default"), "Style dir missing in DESTDIR"
    assert os.path.isdir(f"{destdir}/usr/share/kynex/qml"), "QML dir missing in DESTDIR"
    assert os.path.isfile(f"{destdir}/usr/local/bin/kynex-daemon"), "Daemon missing in DESTDIR"
    print(f"[QML E2E] DESTDIR staged successfully.")
    return destdir


def main():
    destdir = stage_destdir()
    tmp_dir = tempfile.mkdtemp(prefix="kynex-qml-replay-")
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

    daemon_bin = f"{destdir}/usr/local/bin/kynex-daemon"
    daemon_log = open(os.path.join(tmp_dir, "daemon.log"), "w")
    daemon_proc = subprocess.Popen([daemon_bin], env=env, stdout=daemon_log, stderr=subprocess.STDOUT)
    time.sleep(0.3)

    geometries = [
        "1920x1080@1",
        "2520x1400@2",
        "3840x2160@2",
    ]

    harness_bin = str(ROOT_DIR / "target/debug/qml_taskbar_harness")
    overall_rc = 0

    bus = dbus.bus.BusConnection(bus_address)
    feed_obj = bus.get_object("org.kynexos.Daemon", "/org/kynexos/Daemon/Windows", introspect=False)
    feeder_iface = dbus.Interface(feed_obj, "org.kynexos.Daemon.WindowsFeed1")

    for geom in geometries:
        print(f"\n=======================================================")
        print(f"[QML E2E] Running Geometry {geom}")
        print(f"=======================================================")
        # Reset windows before each geometry
        feeder_iface.Reset()
        time.sleep(0.1)

        cmd = [harness_bin, "--destdir", destdir, "--geometry", geom]
        print(f"[QML E2E] Executing: {' '.join(cmd)}")
        sys.stdout.flush()
        res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR))
        if res.returncode != 0:
            print(f"[QML E2E] Geometry {geom} FAILED with code {res.returncode}")
            overall_rc = res.returncode
            break
        print(f"[QML E2E] Geometry {geom} PASSED")

    # If all geometries passed and mutants are requested or default
    run_mutants = "--test-mutants" in sys.argv or "--with-mutants" in sys.argv or len(sys.argv) > 1 and sys.argv[1] == "mutants"
    if overall_rc == 0 and run_mutants:
        print(f"\n=======================================================")
        print(f"[QML E2E] Running Named Mutants (R4.3)")
        print(f"=======================================================")

        mutants = [
            ("MUTANT-NO-NOTIFY", {"KYNEX_TEST_MUTANT_NO_NOTIFY": "1"}, "did not emit rowsInserted, dataChanged, or modelReset"),
            ("MUTANT-ZERO-WIDTH", {"KYNEX_TEST_MUTANT_ZERO_WIDTH": "1"}, "dimensions are 0x"),
            ("MUTANT-HIDDEN-DELEGATE", {"KYNEX_TEST_MUTANT_HIDDEN_DELEGATE": "1"}, "isVisible() is false"),
        ]

        for mutant_name, mutant_env, expected_err in mutants:
            print(f"\n[QML E2E] Testing {mutant_name}...")
            feeder_iface.Reset()
            time.sleep(0.1)

            m_env = env.copy()
            m_env.update(mutant_env)
            cmd = [harness_bin, "--destdir", destdir, "--geometry", "1920x1080@1"]
            res = subprocess.run(cmd, env=m_env, cwd=str(ROOT_DIR), capture_output=True, text=True)
            output = res.stdout + "\n" + res.stderr
            if res.returncode != 0 and expected_err in output:
                print(f"[QML E2E] {mutant_name} KILLED as expected (exit code {res.returncode}, matched: '{expected_err}').")
            else:
                print(f"[QML E2E] ERROR: {mutant_name} SURVIVED! Exit code: {res.returncode}\nOutput:\n{output}", file=sys.stderr)
                overall_rc = 1
                break

    # Cleanup
    daemon_proc.terminate()
    try:
        daemon_proc.wait(timeout=2.0)
    except subprocess.TimeoutExpired:
        daemon_proc.kill()

    dbus_proc.terminate()
    try:
        dbus_proc.wait(timeout=1.0)
    except subprocess.TimeoutExpired:
        dbus_proc.kill()

    if overall_rc != 0:
        daemon_log_path = os.path.join(tmp_dir, "daemon.log")
        if os.path.exists(daemon_log_path):
            print("\n=== DAEMON LOG ===")
            print(open(daemon_log_path).read())
            print("==================\n")

    shutil.rmtree(destdir, ignore_errors=True)
    shutil.rmtree(tmp_dir, ignore_errors=True)

    sys.exit(overall_rc)


if __name__ == "__main__":
    main()
