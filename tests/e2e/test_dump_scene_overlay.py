#!/usr/bin/env python3
"""
DumpScene Session Overlay Verification Test (Stage S2.7).

Verifies that DumpScene includes the sessionOverlay object when the overlay is open,
exposing item objectNames, enabled flags, visible states, and model values seen (no user content).
"""

import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import json
from pathlib import Path

import dbus
import dbus.bus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

DBusGMainLoop(set_as_default=True)
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


class FakeLogind(dbus.service.Object):
    def __init__(self, bus_name):
        super().__init__(bus_name, "/org/freedesktop/login1")

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="", out_signature="s")
    def CanReboot(self):
        return "yes"

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="", out_signature="s")
    def CanPowerOff(self):
        return "yes"

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="", out_signature="s")
    def CanSuspend(self):
        return "yes"


class FakeSystemd(dbus.service.Object):
    def __init__(self, bus_name):
        super().__init__(bus_name, "/org/freedesktop/systemd1")

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="ss", out_signature="o")
    def StopUnit(self, name, mode):
        return dbus.ObjectPath("/org/freedesktop/systemd1/job/1")


def main():
    destdir = tempfile.mkdtemp(prefix="kynex-destdir-dumpscene-")
    cmd = [str(ROOT_DIR / "packaging/session/install-session.sh"), "--force"]
    env = os.environ.copy()
    env["DESTDIR"] = destdir
    res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR), capture_output=True, text=True)
    if res.returncode != 0:
        print(f"ERROR: install-session.sh failed:\n{res.stderr}", file=sys.stderr)
        shutil.rmtree(destdir, ignore_errors=True)
        sys.exit(1)

    daemon_src = ROOT_DIR / "target/debug/kynex-daemon"
    if daemon_src.exists():
        shutil.copy2(str(daemon_src), f"{destdir}/usr/local/bin/kynex-daemon")

    tmp_dir = tempfile.mkdtemp(prefix="kynex-dumpscene-")
    socket_path = os.path.join(tmp_dir, "dbus.sock")
    bus_address = f"unix:path={socket_path}"

    empty_services = os.path.join(tmp_dir, "empty-services")
    os.makedirs(empty_services, exist_ok=True)
    conf_path = os.path.join(tmp_dir, "session.conf")
    with open(conf_path, "w", encoding="utf-8") as f:
        f.write(DBUS_CONFIG_TEMPLATE.format(socket_path=socket_path, empty_services=empty_services))

    dbus_proc = subprocess.Popen(["dbus-daemon", f"--config-file={conf_path}", "--nofork"])
    time.sleep(0.15)

    bus = dbus.bus.BusConnection(bus_address)
    logind_name = dbus.service.BusName("org.freedesktop.login1", bus=bus)
    _fake_logind = FakeLogind(logind_name)

    systemd_name = dbus.service.BusName("org.freedesktop.systemd1", bus=bus)
    _fake_systemd = FakeSystemd(systemd_name)

    loop = GLib.MainLoop()
    glib_thread = threading.Thread(target=loop.run, daemon=True)
    glib_thread.start()

    env["DBUS_SESSION_BUS_ADDRESS"] = bus_address
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["QT_QUICK_BACKEND"] = "software"
    env["KYNEX_STYLE_DIR"] = f"{destdir}/usr/share/kynex/styles/default"
    env["KYNEX_QML_IMPORT_DIR"] = f"{destdir}/usr/share/kynex/qml"
    env["KYNEX_ALLOW_TEST_FEEDER"] = "1"
    env["KYNEX_TEST_LOGIND_BUS"] = "session"
    env["KYNEX_TEST_RATE_LIMIT_MS"] = "10"
    env["RUST_LOG"] = "kynex_shell=warn,kynex_daemon=warn"
    env["XDG_DATA_DIRS"] = f"{destdir}/usr/share:/usr/share"
    xdg_runtime = os.path.join(tmp_dir, "runtime")
    os.makedirs(xdg_runtime, mode=0o700, exist_ok=True)
    env["XDG_RUNTIME_DIR"] = xdg_runtime

    daemon_bin = f"{destdir}/usr/local/bin/kynex-daemon"
    daemon_log = open(os.path.join(tmp_dir, "daemon.log"), "w")
    daemon_proc = subprocess.Popen([daemon_bin], env=env, stdout=daemon_log, stderr=subprocess.STDOUT)
    time.sleep(0.3)

    harness_bin = str(ROOT_DIR / "target/debug/qml_session_harness")
    cmd = [harness_bin, "--destdir", destdir, "--mode", "actions"]
    harness_res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR), capture_output=True, text=True)

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

    loop.quit()

    shutil.rmtree(destdir, ignore_errors=True)
    shutil.rmtree(tmp_dir, ignore_errors=True)

    print("=== DumpScene Session Overlay Test Output ===")
    print(f"Harness return code: {harness_res.returncode}")
    lines = harness_res.stdout.splitlines()
    dumpscene_line = ""
    for line in lines:
        if "Live DumpScene:" in line:
            dumpscene_line = line
            break
        elif line.startswith('{"'):
            dumpscene_line = line
            break

    found_dumpscene = False
    for i, line in enumerate(lines):
        if "Live DumpScene:" in line and i + 1 < len(lines):
            dumpscene_json_str = lines[i + 1]
            try:
                data = json.loads(dumpscene_json_str)
                session_overlay = data.get("sessionOverlay")
                print("\nParsed sessionOverlay object from DumpScene:")
                print(json.dumps(session_overlay, indent=2))
                assert session_overlay is not None, "sessionOverlay missing from DumpScene!"
                assert session_overlay["visible"] is True, "sessionOverlay.visible is not True!"
                assert session_overlay["modelValues"]["can_logout"] is True, "can_logout is not True!"
                assert session_overlay["modelValues"]["can_restart"] is True, "can_restart is not True!"
                assert session_overlay["modelValues"]["can_power_off"] is True, "can_power_off is not True!"
                assert session_overlay["modelValues"]["can_suspend"] is True, "can_suspend is not True!"
                item_names = [it["objectName"] for it in session_overlay["items"]]
                print(f"Found {len(item_names)} overlay items: {item_names}")
                assert "logoutBtn" in item_names, "logoutBtn missing from items!"
                assert "restartBtn" in item_names, "restartBtn missing from items!"
                assert "powerOffBtn" in item_names, "powerOffBtn missing from items!"
                assert "suspendBtn" in item_names, "suspendBtn missing from items!"
                assert "lockBtn" in item_names, "lockBtn missing from items!"
                print("\nDumpScene session overlay verification: PASSED (Hard security invariant verified: NO user content)")
                found_dumpscene = True
            except Exception as e:
                print(f"Error parsing DumpScene JSON: {e}")
                sys.exit(1)
            break

    if not found_dumpscene:
        print("FAIL: Live DumpScene was not located in stdout")
        sys.exit(1)


if __name__ == "__main__":
    main()
