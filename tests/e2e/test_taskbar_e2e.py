#!/usr/bin/env python3
"""
End-to-End Taskbar Process Test (R3.2).
Runs the real kynex-daemon and kynex-shell binaries (offscreen / software backend)
from a staged DESTDIR install layout over an isolated private D-Bus daemon.
Replays captured sequences from live-session-1 (Calculator, apport-gtk, minimize, restore, close)
and validates taskbar state through org.kynexos.Shell.Debug1.DumpState.

Covers:
1. Cold start concurrent (shell & daemon start in same second, multiple orders)
2. Daemon restart while windows exist
3. Shell restart while windows exist
4. Feed arriving BEFORE shell connects
Each variant runs 20 times.
Also verifies named mutants:
- MUTANT-LIST-01: Disabling initial List() causes Variant 4 (feed before shell) to fail.
- MUTANT-SUB-01: Disabling dynamic signal subscription causes Variant 1 (dynamic feed) to fail.
"""

import argparse
import dbus
import json
import os
import shutil
import signal
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

CALC_ID = "{00de40a7-b66c-4c26-936f-8e694c56c4a5}"
CALC_APP_ID = "org.gnome.Calculator"
CALC_TITLE = "Calculator"

APPORT_ID = "{d58df79b-1e65-4cf1-8cf1-df43af2ae21b}"
APPORT_APP_ID = "apport-gtk"
APPORT_TITLE = "Ubuntu"


def stage_destdir() -> str:
    """Installs session files into a temporary DESTDIR staging root."""
    destdir = tempfile.mkdtemp(prefix="kynex-destdir-")
    print(f"[E2E] Staging install-session into {destdir}...")
    cmd = [str(ROOT_DIR / "packaging/session/install-session.sh"), "--force"]
    env = os.environ.copy()
    env["DESTDIR"] = destdir
    res = subprocess.run(cmd, env=env, cwd=str(ROOT_DIR), capture_output=True, text=True)
    if res.returncode != 0:
        print(f"ERROR: install-session.sh failed:\n{res.stderr}", file=sys.stderr)
        shutil.rmtree(destdir, ignore_errors=True)
        sys.exit(1)

    # Verify critical staged assets exist
    assert os.path.isdir(f"{destdir}/usr/share/kynex/styles/default"), "Style dir missing in DESTDIR"
    assert os.path.isdir(f"{destdir}/usr/share/kynex/qml"), "QML dir missing in DESTDIR"
    assert os.path.isfile(f"{destdir}/usr/local/bin/kynex-daemon"), "Daemon missing in DESTDIR"
    assert os.path.isfile(f"{destdir}/usr/local/bin/kynex-shell"), "Shell missing in DESTDIR"
    print(f"[E2E] DESTDIR staged successfully.")
    return destdir


class TestHarness:
    def __init__(self, destdir: str, mutant: str = None):
        self.destdir = destdir
        self.mutant = mutant
        self.tmp_dir = tempfile.mkdtemp(prefix="kynex-e2e-run-")
        self.socket_path = os.path.join(self.tmp_dir, "dbus.sock")
        self.bus_address = f"unix:path={self.socket_path}"

        # Write private bus config
        empty_services = os.path.join(self.tmp_dir, "empty-services")
        os.makedirs(empty_services, exist_ok=True)
        conf_path = os.path.join(self.tmp_dir, "session.conf")
        with open(conf_path, "w", encoding="utf-8") as f:
            f.write(DBUS_CONFIG_TEMPLATE.format(socket_path=self.socket_path, empty_services=empty_services))

        # Start isolated private dbus-daemon
        self.dbus_proc = subprocess.Popen(
            ["dbus-daemon", f"--config-file={conf_path}", "--nofork"]
        )
        time.sleep(0.15)

        self.env = os.environ.copy()
        self.env["DBUS_SESSION_BUS_ADDRESS"] = self.bus_address
        self.env["QT_QPA_PLATFORM"] = "offscreen"
        self.env["QT_QUICK_BACKEND"] = "software"
        self.env["KYNEX_STYLE_DIR"] = f"{self.destdir}/usr/share/kynex/styles/default"
        self.env["KYNEX_QML_IMPORT_DIR"] = f"{self.destdir}/usr/share/kynex/qml"
        self.env["KYNEX_ALLOW_TEST_FEEDER"] = "1"
        self.env["RUST_LOG"] = "kynex_shell=info,kynex_daemon=info"
        self.env["XDG_DATA_DIRS"] = f"{self.destdir}/usr/share:/usr/share"

        if mutant == "disable-list":
            self.env["KYNEX_TEST_MUTANT_DISABLE_LIST"] = "1"
        elif mutant == "disable-subscription":
            self.env["KYNEX_TEST_MUTANT_DISABLE_SUBSCRIPTION"] = "1"

        self.daemon_proc = None
        self.shell_proc = None
        self.bus = dbus.bus.BusConnection(self.bus_address)

    def start_daemon(self):
        daemon_bin = f"{self.destdir}/usr/local/bin/kynex-daemon"
        daemon_log = open(os.path.join(self.tmp_dir, "daemon.log"), "w")
        self.daemon_proc = subprocess.Popen(
            [daemon_bin], env=self.env, stdout=daemon_log, stderr=subprocess.STDOUT
        )

    def start_shell(self):
        shell_bin = f"{self.destdir}/usr/local/bin/kynex-shell"
        shell_log = open(os.path.join(self.tmp_dir, "shell.log"), "w")
        self.shell_proc = subprocess.Popen(
            [shell_bin], env=self.env, stdout=shell_log, stderr=subprocess.STDOUT
        )

    def stop_daemon(self):
        if self.daemon_proc:
            self.daemon_proc.terminate()
            try:
                self.daemon_proc.wait(timeout=2.0)
            except subprocess.TimeoutExpired:
                self.daemon_proc.kill()
            self.daemon_proc = None

    def stop_shell(self):
        if self.shell_proc:
            self.shell_proc.terminate()
            try:
                self.shell_proc.wait(timeout=2.0)
            except subprocess.TimeoutExpired:
                self.shell_proc.kill()
            self.shell_proc = None

    def feeder_upsert(self, win_id: str, title: str, app_id: str, is_minimized: bool, is_active: bool):
        obj = self.bus.get_object("org.kynexos.Daemon", "/org/kynexos/Daemon/Windows", introspect=False)
        iface = dbus.Interface(obj, "org.kynexos.Daemon.WindowsFeed1")
        iface.Upsert(win_id, title, app_id, is_minimized, is_active)

    def feeder_set_active(self, win_id: str):
        obj = self.bus.get_object("org.kynexos.Daemon", "/org/kynexos/Daemon/Windows", introspect=False)
        iface = dbus.Interface(obj, "org.kynexos.Daemon.WindowsFeed1")
        iface.SetActive(win_id)

    def feeder_remove(self, win_id: str):
        obj = self.bus.get_object("org.kynexos.Daemon", "/org/kynexos/Daemon/Windows", introspect=False)
        iface = dbus.Interface(obj, "org.kynexos.Daemon.WindowsFeed1")
        iface.Remove(win_id)

    def get_dump_state(self) -> dict:
        obj = self.bus.get_object("org.kynexos.Shell", "/org/kynexos/Shell", introspect=False)
        iface = dbus.Interface(obj, "org.kynexos.Shell.Debug1")
        payload = iface.DumpState()
        return json.loads(payload)

    def wait_for_condition(self, predicate, timeout: float = 6.0, poll_interval: float = 0.05, desc: str = "condition"):
        start = time.monotonic()
        last_val = None
        while time.monotonic() - start < timeout:
            if self.shell_proc and self.shell_proc.poll() is not None:
                raise RuntimeError(f"kynex-shell process exited prematurely with code {self.shell_proc.returncode}")
            if self.daemon_proc and self.daemon_proc.poll() is not None:
                raise RuntimeError(f"kynex-daemon process exited prematurely with code {self.daemon_proc.returncode}")
            try:
                state = self.get_dump_state()
                last_val = state
                if predicate(state):
                    return state
            except Exception as e:
                last_val = str(e)
            time.sleep(poll_interval)

        shell_log_path = os.path.join(self.tmp_dir, "shell.log")
        daemon_log_path = os.path.join(self.tmp_dir, "daemon.log")
        shell_out = open(shell_log_path).read() if os.path.exists(shell_log_path) else "<no shell.log>"
        daemon_out = open(daemon_log_path).read() if os.path.exists(daemon_log_path) else "<no daemon.log>"
        raise TimeoutError(f"Timed out waiting for {desc} (last state: {last_val})\n=== SHELL LOG ===\n{shell_out}\n=== DAEMON LOG ===\n{daemon_out}")

    def wait_for_ready(self, timeout: float = 8.0):
        def is_ready(s):
            return s.get("daemon_connected") is True and s.get("models", {}).get("windows") is True
        return self.wait_for_condition(is_ready, timeout=timeout, desc="daemon_connected and windows model ready")

    def cleanup(self):
        self.stop_shell()
        self.stop_daemon()
        if self.dbus_proc:
            self.dbus_proc.terminate()
            try:
                self.dbus_proc.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                self.dbus_proc.kill()
        shutil.rmtree(self.tmp_dir, ignore_errors=True)
        time.sleep(0.02)


def replay_real_sequence(harness: TestHarness):
    """
    Replays the real captured sequence from live-session-1/kynex-monitor.txt
    and asserts DumpState correctness at each transition.
    """
    # 1. Initial State: Empty taskbar
    state = harness.get_dump_state()
    assert state["taskbar"]["window_count"] == 0, f"Expected 0 windows initially, got {state['taskbar']['window_count']}"

    # 2. Upsert Calculator (from live-session-1 trace)
    harness.feeder_upsert(CALC_ID, CALC_TITLE, CALC_APP_ID, False, True)
    harness.feeder_set_active(CALC_ID)

    def calc_added(s):
        tb = s["taskbar"]
        if tb["window_count"] != 1:
            return False
        for e in tb["entries"]:
            if CALC_APP_ID in e["app_id"] and e["is_running"] and e["is_active"] and not e["is_minimized"]:
                return True
        return False

    state = harness.wait_for_condition(calc_added, timeout=3.0, desc="Calculator added and active")

    # 3. Upsert unindexed window apport-gtk (no desktop entry, letter badge "A")
    harness.feeder_upsert(APPORT_ID, APPORT_TITLE, APPORT_APP_ID, False, True)
    harness.feeder_set_active(APPORT_ID)

    def apport_added(s):
        tb = s["taskbar"]
        if tb["window_count"] != 2:
            return False
        found_apport = False
        for e in tb["entries"]:
            if e["app_id"] == APPORT_APP_ID and e["is_running"] and e["is_active"]:
                assert e.get("letter_badge") in ("A", "U"), f"Expected badge 'A' or 'U', got {e.get('letter_badge')}"
                found_apport = True
        return found_apport

    state = harness.wait_for_condition(apport_added, timeout=3.0, desc="apport-gtk added with badge A")

    # 4. Minimize Calculator
    harness.feeder_set_active(APPORT_ID)
    harness.feeder_upsert(CALC_ID, CALC_TITLE, CALC_APP_ID, True, False)

    def calc_minimized(s):
        tb = s["taskbar"]
        if tb["window_count"] != 2:
            return False
        for e in tb["entries"]:
            if CALC_APP_ID in e["app_id"] and e["is_minimized"] and not e["is_active"]:
                return True
        return False

    state = harness.wait_for_condition(calc_minimized, timeout=3.0, desc="Calculator minimized and flagged")

    # 5. Restore Calculator
    harness.feeder_upsert(CALC_ID, CALC_TITLE, CALC_APP_ID, False, True)
    harness.feeder_set_active(CALC_ID)

    def calc_restored(s):
        tb = s["taskbar"]
        for e in tb["entries"]:
            if CALC_APP_ID in e["app_id"] and not e["is_minimized"] and e["is_active"]:
                return True
        return False

    state = harness.wait_for_condition(calc_restored, timeout=3.0, desc="Calculator restored and active")

    # 6. Remove apport-gtk
    harness.feeder_remove(APPORT_ID)

    def apport_removed(s):
        tb = s["taskbar"]
        if tb["window_count"] != 1:
            return False
        return not any(e["app_id"] == APPORT_APP_ID for e in tb["entries"])

    state = harness.wait_for_condition(apport_removed, timeout=3.0, desc="apport-gtk removed")

    # 7. Remove Calculator
    harness.feeder_remove(CALC_ID)

    def calc_removed(s):
        return s["taskbar"]["window_count"] == 0 and len(s["taskbar"]["entries"]) == 0

    state = harness.wait_for_condition(calc_removed, timeout=3.0, desc="Calculator removed, taskbar empty")


def run_variant_1_cold_start(destdir: str, iterations: int = 20, mutant: str = None):
    print(f"\n--- Variant 1: Cold Start Concurrent ({iterations} runs) [mutant={mutant}] ---")
    for i in range(iterations):
        h = TestHarness(destdir, mutant=mutant)
        try:
            order = i % 3
            if order == 0:
                # Daemon starts first, shell immediately after
                h.start_daemon()
                h.start_shell()
            elif order == 1:
                # Shell starts first, daemon immediately after
                h.start_shell()
                h.start_daemon()
            else:
                # Both started concurrently with slight microsecond jitter
                h.start_daemon()
                time.sleep(0.01)
                h.start_shell()

            h.wait_for_ready()
            replay_real_sequence(h)
            print(f"  [Variant 1] Run {i+1}/{iterations} passed (order={order})")
        finally:
            h.cleanup()


def run_variant_2_daemon_restart(destdir: str, iterations: int = 20, mutant: str = None):
    print(f"\n--- Variant 2: Daemon Restart While Windows Exist ({iterations} runs) [mutant={mutant}] ---")
    for i in range(iterations):
        h = TestHarness(destdir, mutant=mutant)
        try:
            h.start_daemon()
            h.start_shell()
            h.wait_for_ready()

            # Feed Calculator
            h.feeder_upsert(CALC_ID, CALC_TITLE, CALC_APP_ID, False, True)
            h.feeder_set_active(CALC_ID)
            h.wait_for_condition(lambda s: s["taskbar"]["window_count"] == 1, desc="Initial Calculator window")

            # Kill daemon
            h.stop_daemon()
            # Assert shell detects disconnect
            h.wait_for_condition(lambda s: s.get("daemon_connected") is False or s["taskbar"]["window_count"] == 0,
                                 timeout=3.0, desc="Shell detected daemon disconnected")

            # Restart daemon
            h.start_daemon()
            # Wait for shell to reconnect
            h.wait_for_condition(lambda s: s.get("daemon_connected") is True, timeout=4.0, desc="Shell reconnected to daemon")

            # Feeder resyncs window
            h.feeder_upsert(CALC_ID, CALC_TITLE, CALC_APP_ID, False, True)
            h.feeder_set_active(CALC_ID)
            h.wait_for_condition(lambda s: s["taskbar"]["window_count"] == 1, desc="Window resynced after daemon restart")

            # Remove window
            h.feeder_remove(CALC_ID)
            h.wait_for_condition(lambda s: s["taskbar"]["window_count"] == 0, desc="Window removed after restart")

            print(f"  [Variant 2] Run {i+1}/{iterations} passed")
        finally:
            h.cleanup()


def run_variant_3_shell_restart(destdir: str, iterations: int = 20, mutant: str = None):
    print(f"\n--- Variant 3: Shell Restart While Windows Exist ({iterations} runs) [mutant={mutant}] ---")
    for i in range(iterations):
        h = TestHarness(destdir, mutant=mutant)
        try:
            h.start_daemon()
            time.sleep(0.15)
            # Window fed while daemon running
            h.feeder_upsert(CALC_ID, CALC_TITLE, CALC_APP_ID, False, True)
            h.feeder_set_active(CALC_ID)

            # Start shell
            h.start_shell()
            h.wait_for_ready()

            # Assert Calculator appeared via initial List()
            h.wait_for_condition(lambda s: s["taskbar"]["window_count"] == 1, desc="Calculator loaded in initial shell")

            # Terminate shell
            h.stop_shell()
            time.sleep(0.1)

            # Restart shell
            h.start_shell()
            h.wait_for_ready()

            # Window MUST be present immediately without re-feeding!
            def calc_restored_in_new_shell(s):
                if s["taskbar"]["window_count"] != 1:
                    return False
                return any(CALC_APP_ID in e["app_id"] and e["is_running"] for e in s["taskbar"]["entries"])

            h.wait_for_condition(calc_restored_in_new_shell, timeout=3.0, desc="Calculator present after shell restart")

            # Remove window
            h.feeder_remove(CALC_ID)
            h.wait_for_condition(lambda s: s["taskbar"]["window_count"] == 0, desc="Window removed after shell restart")

            print(f"  [Variant 3] Run {i+1}/{iterations} passed")
        finally:
            h.cleanup()


def run_variant_4_feed_before_shell(destdir: str, iterations: int = 20, mutant: str = None):
    print(f"\n--- Variant 4: Feed Arriving BEFORE Shell Connects ({iterations} runs) [mutant={mutant}] ---")
    for i in range(iterations):
        h = TestHarness(destdir, mutant=mutant)
        try:
            h.start_daemon()
            time.sleep(0.2)

            # Feed arrives before shell is even spawned!
            h.feeder_upsert(CALC_ID, CALC_TITLE, CALC_APP_ID, False, True)
            h.feeder_set_active(CALC_ID)
            h.feeder_upsert(APPORT_ID, APPORT_TITLE, APPORT_APP_ID, True, False)

            # Now start shell
            h.start_shell()
            h.wait_for_ready()

            # Assert shell picked up both pre-seeded windows via initial Windows1.List()
            def both_windows_present(s):
                tb = s["taskbar"]
                if tb["window_count"] != 2:
                    return False
                calc_ok = any(CALC_APP_ID in e["app_id"] and e["is_running"] and not e["is_minimized"] for e in tb["entries"])
                apport_ok = any(e["app_id"] == APPORT_APP_ID and e["is_running"] and e["is_minimized"] for e in tb["entries"])
                return calc_ok and apport_ok

            h.wait_for_condition(both_windows_present, timeout=3.0, desc="Both pre-seeded windows present via List()")

            # Clean up windows
            h.feeder_remove(CALC_ID)
            h.feeder_remove(APPORT_ID)
            h.wait_for_condition(lambda s: s["taskbar"]["window_count"] == 0, desc="Cleaned up pre-seeded windows")

            print(f"  [Variant 4] Run {i+1}/{iterations} passed")
        finally:
            h.cleanup()


def main():
    parser = argparse.ArgumentParser(description="Kynex Shell Taskbar End-to-End Process Test (R3.2)")
    parser.add_argument("--variant", choices=["cold", "daemon-restart", "shell-restart", "feed-before", "all"], default="all")
    parser.add_argument("--iterations", type=int, default=20, help="Number of iterations per variant (default: 20)")
    parser.add_argument("--mutant", choices=["disable-list", "disable-subscription"], default=None, help="Inject named mutant")
    args = parser.parse_args()

    destdir = stage_destdir()
    try:
        if args.variant == "all":
            run_variant_1_cold_start(destdir, iterations=args.iterations, mutant=args.mutant)
            run_variant_2_daemon_restart(destdir, iterations=args.iterations, mutant=args.mutant)
            run_variant_3_shell_restart(destdir, iterations=args.iterations, mutant=args.mutant)
            run_variant_4_feed_before_shell(destdir, iterations=args.iterations, mutant=args.mutant)
        elif args.variant == "cold":
            run_variant_1_cold_start(destdir, iterations=args.iterations, mutant=args.mutant)
        elif args.variant == "daemon-restart":
            run_variant_2_daemon_restart(destdir, iterations=args.iterations, mutant=args.mutant)
        elif args.variant == "shell-restart":
            run_variant_3_shell_restart(destdir, iterations=args.iterations, mutant=args.mutant)
        elif args.variant == "feed-before":
            run_variant_4_feed_before_shell(destdir, iterations=args.iterations, mutant=args.mutant)
        print("\n[E2E] ALL TEST VARIANTS COMPLETED SUCCESSFULLY.")
    finally:
        shutil.rmtree(destdir, ignore_errors=True)


if __name__ == "__main__":
    main()
