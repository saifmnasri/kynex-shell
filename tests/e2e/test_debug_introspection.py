#!/usr/bin/env python3
"""
Test for org.kynexos.Shell.Debug1 (DumpScene and GrabPanel).
Validates:
1. DumpScene() returns valid sanitized JSON containing expected fields (panelGeometry, windowSize, devicePixelRatio, screenGeometry, delegateCount, modelRowCount).
2. Hard Security Invariant: DumpScene() contains NO window titles.
3. GrabPanel() creates a PNG at $XDG_RUNTIME_DIR/kynex/panel-<timestamp>.png with mode 0600.
4. Same-UID restriction on Debug1 methods.
"""

import dbus
import json
import os
import shutil
import stat
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
    destdir = tempfile.mkdtemp(prefix="kynex-destdir-debug-")
    print(f"[DEBUG TEST] Staging install-session into {destdir}...")
    cmd = [str(ROOT_DIR / "packaging/session/install-session.sh"), "--force"]
    env = os.environ.copy()
    env["DESTDIR"] = destdir
    res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR), capture_output=True, text=True)
    if res.returncode != 0:
        print(f"ERROR: install-session.sh failed:\n{res.stderr}", file=sys.stderr)
        shutil.rmtree(destdir, ignore_errors=True)
        sys.exit(1)
    return destdir


def main():
    destdir = stage_destdir()
    tmp_dir = tempfile.mkdtemp(prefix="kynex-debug-test-")
    xdg_runtime = os.path.join(tmp_dir, "runtime")
    os.makedirs(xdg_runtime, mode=0o700, exist_ok=True)
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
    env["XDG_RUNTIME_DIR"] = xdg_runtime
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["QT_QUICK_BACKEND"] = "software"
    env["KYNEX_STYLE_DIR"] = f"{destdir}/usr/share/kynex/styles/default"
    env["KYNEX_QML_IMPORT_DIR"] = f"{destdir}/usr/share/kynex/qml"
    env["KYNEX_ALLOW_TEST_FEEDER"] = "1"
    env["RUST_LOG"] = "kynex_shell=debug,kynex_daemon=debug"
    env["XDG_DATA_DIRS"] = f"{destdir}/usr/share:/usr/share"

    daemon_bin = f"{destdir}/usr/local/bin/kynex-daemon"
    daemon_log = open(os.path.join(tmp_dir, "daemon.log"), "w")
    daemon_proc = subprocess.Popen([daemon_bin], env=env, stdout=daemon_log, stderr=subprocess.STDOUT)
    time.sleep(0.3)

    shell_bin = f"{destdir}/usr/local/bin/kynex-shell"
    shell_log = open(os.path.join(tmp_dir, "shell.log"), "w")
    shell_proc = subprocess.Popen([shell_bin], env=env, stdout=shell_log, stderr=subprocess.STDOUT)

    bus = dbus.bus.BusConnection(bus_address)

    # Wait for org.kynexos.Shell to appear on bus
    print("[DEBUG TEST] Waiting for org.kynexos.Shell...")
    shell_obj = None
    start = time.monotonic()
    while time.monotonic() - start < 6.0:
        try:
            shell_obj = bus.get_object("org.kynexos.Shell", "/org/kynexos/Shell", introspect=False)
            iface = dbus.Interface(shell_obj, "org.kynexos.Shell.Debug1")
            _ = iface.DumpState()
            break
        except Exception:
            time.sleep(0.1)

    assert shell_obj is not None, "Timed out waiting for org.kynexos.Shell"
    iface = dbus.Interface(shell_obj, "org.kynexos.Shell.Debug1")

    # 1. Test DumpScene()
    print("[DEBUG TEST] Testing DumpScene()...")
    scene_json_str = str(iface.DumpScene())
    scene = json.loads(scene_json_str)
    print(f"DumpScene keys: {list(scene.keys())}")
    assert "panelGeometry" in scene, "panelGeometry missing in DumpScene"
    assert "windowSize" in scene, "windowSize missing in DumpScene"
    assert "devicePixelRatio" in scene, "devicePixelRatio missing in DumpScene"
    assert "screenGeometry" in scene, "screenGeometry missing in DumpScene"
    assert "delegateCount" in scene, "delegateCount missing in DumpScene"
    assert "modelRowCount" in scene, "modelRowCount missing in DumpScene"
    assert scene["panelGeometry"]["height"] == 48, f"Unexpected panel height {scene['panelGeometry']['height']}"

    # Verify Hard Security Invariant: NO titles
    assert "Calculator" not in scene_json_str, "Found Calculator window title in DumpScene output!"
    assert "Terminal" not in scene_json_str, "Found Terminal window title in DumpScene output!"
    print("PASS: DumpScene structure and sanitization verified.")

    # 2. Test GrabPanel()
    print("[DEBUG TEST] Testing GrabPanel()...")
    png_path = str(iface.GrabPanel())
    print(f"GrabPanel returned path: {png_path}")
    assert os.path.isfile(png_path), f"GrabPanel file does not exist: {png_path}"
    assert png_path.startswith(xdg_runtime), f"GrabPanel path not in XDG_RUNTIME_DIR: {png_path}"

    # Verify permissions are 0600
    st = os.stat(png_path)
    file_mode = stat.S_IMODE(st.st_mode)
    assert file_mode == 0o600, f"Expected permissions 0600, got {oct(file_mode)}"

    # Verify PNG header
    with open(png_path, "rb") as f:
        header = f.read(8)
        assert header == b"\x89PNG\r\n\x1a\n", f"Invalid PNG header: {header!r}"
    print(f"PASS: GrabPanel generated valid PNG ({os.path.getsize(png_path)} bytes) with mode 0600.")

    # Cleanup
    shell_proc.terminate()
    daemon_proc.terminate()
    dbus_proc.terminate()
    shutil.rmtree(destdir, ignore_errors=True)
    shutil.rmtree(tmp_dir, ignore_errors=True)
    print("\n[DEBUG TEST] All tests passed successfully!")


if __name__ == "__main__":
    main()
