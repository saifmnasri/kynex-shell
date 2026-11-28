#!/usr/bin/env python3
"""
Reproduce Session Overlay QML Failure (Stage S2.1).

Instantiates the REAL styles/default/Shell.qml from an installed-layout temp DESTDIR
with REAL cxx-qt models connected to the real daemon and a fake logind on a private bus.
Opens the overlay through the same code path as the status-area button:
panel.toggleQuickSettings() -> QuickSettings -> powerSessionButton.clicked() -> SessionOverlay.
Asserts that rows are ENABLED when the model reports can_*=true, tooltips are not 'unavailable',
and the overlay has no QML warnings.
"""

import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

import dbus
import dbus.bus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

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
        self.reboot_called = 0
        self.poweroff_called = 0
        self.suspend_called = 0

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="", out_signature="s")
    def CanReboot(self):
        return "yes"

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="", out_signature="s")
    def CanPowerOff(self):
        return "yes"

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="", out_signature="s")
    def CanSuspend(self):
        return "yes"

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="b", out_signature="")
    def Reboot(self, interactive):
        self.reboot_called += 1

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="b", out_signature="")
    def PowerOff(self, interactive):
        self.poweroff_called += 1

    @dbus.service.method("org.freedesktop.login1.Manager", in_signature="b", out_signature="")
    def Suspend(self, interactive):
        self.suspend_called += 1


class FakeSystemd(dbus.service.Object):
    def __init__(self, bus_name):
        super().__init__(bus_name, "/org/freedesktop/systemd1")
        self.stop_unit_called = 0
        self.stopped_unit = ""

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="ss", out_signature="o")
    def StopUnit(self, name, mode):
        self.stop_unit_called += 1
        self.stopped_unit = name
        return dbus.ObjectPath("/org/freedesktop/systemd1/job/1")


def stage_destdir() -> str:
    destdir = tempfile.mkdtemp(prefix="kynex-destdir-session-")
    print(f"[SESSION E2E] Staging install-session into {destdir}...")
    cmd = [str(ROOT_DIR / "packaging/session/install-session.sh"), "--force"]
    env = os.environ.copy()
    env["DESTDIR"] = destdir
    res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR), capture_output=True, text=True)
    if res.returncode != 0:
        print(f"ERROR: install-session.sh failed:\n{res.stderr}", file=sys.stderr)
        shutil.rmtree(destdir, ignore_errors=True)
        sys.exit(1)
    # Ensure daemon binary with test-hooks is installed in destdir
    daemon_src = ROOT_DIR / "target/debug/kynex-daemon"
    if daemon_src.exists():
        shutil.copy2(str(daemon_src), f"{destdir}/usr/local/bin/kynex-daemon")
    return destdir


def main():
    DBusGMainLoop(set_as_default=True)
    destdir = stage_destdir()
    tmp_dir = tempfile.mkdtemp(prefix="kynex-session-e2e-")
    socket_path = os.path.join(tmp_dir, "dbus.sock")
    bus_address = f"unix:path={socket_path}"

    empty_services = os.path.join(tmp_dir, "empty-services")
    os.makedirs(empty_services, exist_ok=True)
    conf_path = os.path.join(tmp_dir, "session.conf")
    with open(conf_path, "w", encoding="utf-8") as f:
        f.write(DBUS_CONFIG_TEMPLATE.format(socket_path=socket_path, empty_services=empty_services))

    dbus_proc = subprocess.Popen(["dbus-daemon", f"--config-file={conf_path}", "--nofork"])
    time.sleep(0.2)

    # Connect to private bus
    bus = dbus.bus.BusConnection(bus_address)
    logind_name = dbus.service.BusName("org.freedesktop.login1", bus=bus)
    _fake_logind = FakeLogind(logind_name)

    systemd_name = dbus.service.BusName("org.freedesktop.systemd1", bus=bus)
    _fake_systemd = FakeSystemd(systemd_name)

    loop = GLib.MainLoop()
    glib_thread = threading.Thread(target=loop.run, daemon=True)
    glib_thread.start()

    env = os.environ.copy()
    env["DBUS_SESSION_BUS_ADDRESS"] = bus_address
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["QT_QUICK_BACKEND"] = "software"
    env["KYNEX_STYLE_DIR"] = f"{destdir}/usr/share/kynex/styles/default"
    env["KYNEX_QML_IMPORT_DIR"] = f"{destdir}/usr/share/kynex/qml"
    env["KYNEX_ALLOW_TEST_FEEDER"] = "1"
    env["KYNEX_TEST_LOGIND_BUS"] = "session"
    env["RUST_LOG"] = "kynex_shell=debug,kynex_daemon=debug"
    env["XDG_DATA_DIRS"] = f"{destdir}/usr/share:/usr/share"
    xdg_runtime = os.path.join(tmp_dir, "runtime")
    os.makedirs(xdg_runtime, mode=0o700, exist_ok=True)
    env["XDG_RUNTIME_DIR"] = xdg_runtime

    daemon_bin = f"{destdir}/usr/local/bin/kynex-daemon"
    daemon_log = open(os.path.join(tmp_dir, "daemon.log"), "w")
    daemon_proc = subprocess.Popen([daemon_bin], env=env, stdout=daemon_log, stderr=subprocess.STDOUT)
    time.sleep(0.4)

    # Build harness binary if needed
    harness_bin = str(ROOT_DIR / "target/debug/qml_session_harness")
    if not os.path.exists(harness_bin):
        print(f"[SESSION E2E] Building qml_session_harness...")
        build_res = subprocess.run(["cargo", "build", "--bin", "qml_session_harness"], cwd=str(ROOT_DIR))
        if build_res.returncode != 0:
            print("ERROR: Failed to build qml_session_harness", file=sys.stderr)
            sys.exit(1)

    print(f"\n=======================================================")
    print(f"[SESSION E2E] Running Reproduction in qml_session_harness")
    print(f"=======================================================")
    mode = "reproduce"
    if len(sys.argv) > 1:
        mode = sys.argv[1]
    cmd = [harness_bin, "--destdir", destdir, "--mode", mode]
    sys.stdout.flush()
    harness_res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR))
    ret_code = harness_res.returncode

    # Teardown
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

    loop.quit()

    shutil.rmtree(destdir, ignore_errors=True)
    shutil.rmtree(tmp_dir, ignore_errors=True)

    print(f"\n[SESSION E2E] Harness exited with return code: {ret_code}")
    sys.exit(ret_code)


if __name__ == "__main__":
    main()
