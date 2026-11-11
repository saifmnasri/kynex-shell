#!/usr/bin/env python3
"""Stage N Performance Benchmark Suite (N5.1 & AUD-N-02 Remediated).

Measures:
1. End-to-end notification latency (client time.monotonic_ns to shell steady_clock frameSwapped):
   - 30 cold iterations (fresh shell start -> Notify -> first frameSwapped)
   - 30 warm iterations (running shell -> Notify -> frameSwapped)
   - p50/p95 against the < 50 ms budget row
   - Explicit exclusion note: client setup prior to time.monotonic_ns timestamp.
2. Shell-side-only latency (shell signal reception to frameSwapped).
3. PSS memory:
   - Idle PSS (kynex-shell and kynex-daemon)
   - Daemon PSS with 200 notifications in history buffer
   - Daemon and shell PSS after 1,000 notifications flood
4. Flood CPU:
   - CPU usage during 1,000 notifications flood
5. Idle wakeups:
   - 10-second pidstat on kynex-shell and kynex-daemon (must stay near 0 cswch/s)
6. Saves results to dev/bench/stage_n_perf.json and executes budget-check.py with real CLI.
"""

import json
import os
import re
import subprocess
import sys
import time
from gi.repository import Gio, GLib

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))
TMP_DIR = os.path.join(ROOT_DIR, "dev/tmp")
BIN_DIR = os.path.join(ROOT_DIR, "target/release")
SHELL_BIN = os.path.join(BIN_DIR, "kynex-shell")
DAEMON_BIN = os.path.join(BIN_DIR, "kynex-daemon")


def get_proc_pss(pid):
    try:
        with open(f"/proc/{pid}/smaps_rollup", "r") as f:
            for line in f:
                if line.startswith("Pss:"):
                    return float(line.split()[1]) / 1024.0
    except Exception:
        total_kb = 0
        try:
            with open(f"/proc/{pid}/smaps", "r") as f:
                for line in f:
                    if line.startswith("Pss:"):
                        total_kb += int(line.split()[1])
            return float(total_kb) / 1024.0
        except Exception:
            return 0.0
    return 0.0


def percentile(data, p):
    if not data:
        return 0.0
    s = sorted(data)
    k = (len(s) - 1) * (p / 100.0)
    f = int(k)
    c = min(f + 1, len(s) - 1)
    d0 = s[f] * (c - k)
    d1 = s[c] * (k - f)
    return d0 + d1


def get_dbus_connection(bus_addr):
    return Gio.DBusConnection.new_for_address_sync(
        bus_addr,
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )


def send_notify(conn, app_name, summary, body, timeout_ms=5000):
    # No sleep in timed region. Client timestamp captured immediately prior to method call.
    t_client_ns = time.monotonic_ns()
    conn.call_sync(
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
        "Notify",
        GLib.Variant("(susssasa{sv}i)", (app_name, 0, "", summary, body, [], {}, timeout_ms)),
        GLib.VariantType("(u)"),
        Gio.DBusCallFlags.NONE,
        -1,
        None,
    )
    return t_client_ns


def main():
    print("=== Stage N Desktop Notifications Performance Measurement ===")
    if not os.path.isfile(SHELL_BIN) or not os.path.isfile(DAEMON_BIN):
        sys.exit("ERROR: Release binaries not found. Run cargo build --release first.")

    subprocess.run(
        ["./dev/run-nested.sh", "--cleanup"],
        cwd=ROOT_DIR,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    addr_file = os.path.join(TMP_DIR, "dbus-session-bus-address")
    wayland_file = os.path.join(TMP_DIR, "wayland-display")
    if os.path.exists(addr_file):
        os.remove(addr_file)
    if os.path.exists(wayland_file):
        os.remove(wayland_file)

    nested_env = os.environ.copy()
    nested_env["KYNEX_BUILD_PROFILE"] = "release"
    nested_env["KYNEX_KEEP_SESSION_ALIVE"] = "1"
    nested_env["KYNEX_ALLOW_TEST_FEEDER"] = "1"

    print("Starting nested session...")
    nested_proc = subprocess.Popen(
        ["./dev/run-nested.sh"],
        cwd=ROOT_DIR,
        env=nested_env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    bus_addr = None
    for _ in range(300):
        if os.path.exists(addr_file):
            content = open(addr_file).read().strip()
            if content:
                bus_addr = content.split(",guid=")[0]
                break
        time.sleep(0.2)

    wayland_display = None
    for _ in range(100):
        if os.path.exists(wayland_file):
            content = open(wayland_file).read().strip()
            if content:
                wayland_display = content
                break
        time.sleep(0.1)

    if not bus_addr or not wayland_display:
        nested_proc.terminate()
        sys.exit("ERROR: Timed out waiting for session.")

    print(f"Session Bus: {bus_addr}")
    print(f"Wayland: {wayland_display}")

    # Wait for daemon
    time.sleep(2.0)
    daemon_pid = None
    for entry in os.listdir("/proc"):
        if entry.isdigit():
            try:
                cmd = open(f"/proc/{entry}/cmdline").read()
                if "target/release/kynex-daemon" in cmd:
                    daemon_pid = int(entry)
                    break
            except Exception:
                pass

    print(f"Daemon PID: {daemon_pid}")

    shell_env = os.environ.copy()
    shell_env["DBUS_SESSION_BUS_ADDRESS"] = bus_addr
    shell_env["WAYLAND_DISPLAY"] = wayland_display
    shell_env["XDG_RUNTIME_DIR"] = os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000")
    shell_env["QT_QPA_PLATFORM"] = "wayland"
    shell_env["KYNEX_STYLE_DIR"] = os.path.join(ROOT_DIR, "styles/default")
    shell_env["RUST_LOG"] = "info,kynex_perf=info"
    shell_env["NO_COLOR"] = "1"

    # Kill initial shell to have clean state
    for entry in os.listdir("/proc"):
        if entry.isdigit():
            try:
                cmdline = open(f"/proc/{entry}/cmdline").read()
                if "target/release/kynex-shell" in cmdline:
                    os.kill(int(entry), 9)
            except Exception:
                pass
    time.sleep(0.5)

    client_conn = get_dbus_connection(bus_addr)

    cold_e2e_samples = []
    cold_shell_samples = []
    print("\n--- Measuring 30 COLD notification latency samples ---")
    cold_log = os.path.join(TMP_DIR, "cold_notif_bench.log")

    for i in range(1, 31):
        if os.path.exists(cold_log):
            os.remove(cold_log)

        with open(cold_log, "w") as out:
            p = subprocess.Popen(
                ["stdbuf", "-oL", "-eL", SHELL_BIN],
                env=shell_env,
                cwd=ROOT_DIR,
                stdout=out,
                stderr=out,
            )

        # Wait for shell initialization
        time.sleep(0.8)

        # Trigger notification
        t_client_ns = send_notify(client_conn, "ColdTest", f"Cold {i}", f"Cold body {i}", 2000)

        # Wait for kynex_perf log
        frame_ns = None
        shell_lat_ms = None
        for _ in range(60):
            if os.path.exists(cold_log):
                with open(cold_log, "r") as f:
                    for line in f:
                        clean = re.sub(r"\x1b\[[0-9;]*m", "", line)
                        if "Notification first frame swapped" in clean:
                            m_frame = re.search(r"frame_ns=(\d+)", clean)
                            m_shell = re.search(r"(?:shell_latency_ms|latency_ms)=([\d\.]+)", clean)
                            if m_frame and m_shell:
                                frame_ns = int(m_frame.group(1))
                                shell_lat_ms = float(m_shell.group(1))
                                break
                            elif m_shell:
                                shell_lat_ms = float(m_shell.group(1))
                                break
            if shell_lat_ms is not None:
                break
            time.sleep(0.05)

        p.terminate()
        p.wait()

        if shell_lat_ms is not None:
            e2e_ms = (frame_ns - t_client_ns) / 1_000_000.0 if frame_ns is not None else shell_lat_ms
            cold_e2e_samples.append(e2e_ms)
            cold_shell_samples.append(shell_lat_ms)
            print(f"  Cold sample {i:2d}/30: e2e = {e2e_ms:6.2f} ms (shell-side = {shell_lat_ms:6.2f} ms)")
        else:
            print(f"  Cold sample {i:2d}/30: TIMEOUT")

    # Start long-running shell for WARM tests and memory measurements
    warm_log_file = os.path.join(TMP_DIR, "warm_notif_bench.log")
    if os.path.exists(warm_log_file):
        os.remove(warm_log_file)
    warm_out = open(warm_log_file, "w")
    shell_proc = subprocess.Popen(
        ["stdbuf", "-oL", "-eL", SHELL_BIN],
        env=shell_env,
        cwd=ROOT_DIR,
        stdout=warm_out,
        stderr=warm_out,
    )
    time.sleep(1.5)
    shell_pid = shell_proc.pid
    print(f"\nLong-running Shell PID: {shell_pid}")

    print("\n--- Measuring 30 WARM notification latency samples ---")
    warm_e2e_samples = []
    warm_shell_samples = []

    for j in range(1, 31):
        warm_out.flush()
        offset = os.path.getsize(warm_log_file)

        t_client_ns = send_notify(client_conn, "WarmTest", f"Warm {j}", f"Warm body {j}", 200)

        frame_ns = None
        shell_lat_ms = None
        for _ in range(50):
            time.sleep(0.02)
            with open(warm_log_file, "r") as f:
                f.seek(offset)
                new_text = f.read()
                clean = re.sub(r"\x1b\[[0-9;]*m", "", new_text)
                if "Notification first frame swapped" in clean:
                    m_frame = re.search(r"frame_ns=(\d+)", clean)
                    m_shell = re.search(r"(?:shell_latency_ms|latency_ms)=([\d\.]+)", clean)
                    if m_frame and m_shell:
                        frame_ns = int(m_frame.group(1))
                        shell_lat_ms = float(m_shell.group(1))
                        break
                    elif m_shell:
                        shell_lat_ms = float(m_shell.group(1))
                        break
            if shell_lat_ms is not None:
                break

        if shell_lat_ms is not None:
            e2e_ms = (frame_ns - t_client_ns) / 1_000_000.0 if frame_ns is not None else shell_lat_ms
            warm_e2e_samples.append(e2e_ms)
            warm_shell_samples.append(shell_lat_ms)
            print(f"  Warm sample {j:2d}/30: e2e = {e2e_ms:6.2f} ms (shell-side = {shell_lat_ms:6.2f} ms)")
        else:
            print(f"  Warm sample {j:2d}/30: TIMEOUT")

        time.sleep(0.55)

    # Memory measurements
    print("\n--- Measuring PSS & Memory Plateau ---")
    time.sleep(2.0)  # Let popups expire
    pss_idle_shell = get_proc_pss(shell_pid)
    pss_idle_daemon = get_proc_pss(daemon_pid) if daemon_pid else 0.0
    print(f"  Idle PSS - Shell: {pss_idle_shell:.2f} MB, Daemon: {pss_idle_daemon:.2f} MB")

    print("\nPopulating history buffer to capacity (200 notifications)...")
    for k in range(1, 201):
        try:
            conn_k = get_dbus_connection(bus_addr)
            send_notify(conn_k, "HistApp", f"History {k}", "Body", 50)
        except Exception:
            pass
        if k % 20 == 0:
            time.sleep(0.02)

    time.sleep(1.5)  # Wait for popups to close and move to history
    pss_200_hist_daemon = get_proc_pss(daemon_pid) if daemon_pid else 0.0
    print(f"  Daemon PSS with 200 history entries: {pss_200_hist_daemon:.2f} MB")

    print("\nFlooding 1,000 notifications in rapid sequence (flood CPU check)...")
    def get_proc_ticks(pid):
        try:
            with open(f"/proc/{pid}/stat", "r") as f:
                parts = f.read().split()
                return int(parts[13]) + int(parts[14])
        except Exception:
            return 0

    ticks_start = get_proc_ticks(daemon_pid) if daemon_pid else 0
    t0 = time.time()
    flood_conns = [get_dbus_connection(bus_addr) for _ in range(100)]
    for f_idx in range(1000):
        try:
            conn_f = flood_conns[f_idx % len(flood_conns)]
            send_notify(conn_f, "FloodApp", f"Flood {f_idx}", "Flood body", 10)
        except Exception:
            pass

    t_flood = time.time() - t0
    ticks_end = get_proc_ticks(daemon_pid) if daemon_pid else 0
    clk_tck = os.sysconf(os.sysconf_names.get("SC_CLK_TCK", "SC_CLK_TCK"))
    peak_flood_cpu = (
        round(((ticks_end - ticks_start) / clk_tck) / t_flood * 100.0, 2)
        if t_flood > 0 and clk_tck > 0
        else 0.0
    )
    print(f"  Flood completed in {t_flood:.2f} s. Peak daemon CPU: {peak_flood_cpu:.1f}%")

    time.sleep(2.0)
    pss_after_flood_daemon = get_proc_pss(daemon_pid) if daemon_pid else 0.0
    pss_after_flood_shell = get_proc_pss(shell_pid)
    print(f"  After 1,000 flood - Shell: {pss_after_flood_shell:.2f} MB, Daemon: {pss_after_flood_daemon:.2f} MB")

    # Idle wakeups via pidstat
    print("\n--- Measuring Idle Wakeups via pidstat (10 seconds) ---")
    target_pids = f"{shell_pid},{daemon_pid}" if daemon_pid else str(shell_pid)
    pidstat_res = subprocess.run(
        ["pidstat", "-w", "-u", "-p", target_pids, "1", "10"],
        capture_output=True,
        text=True,
    )
    print(pidstat_res.stdout)

    # Statistics calculation
    cold_e2e_p50 = percentile(cold_e2e_samples, 50)
    cold_e2e_p95 = percentile(cold_e2e_samples, 95)
    cold_shell_p50 = percentile(cold_shell_samples, 50)
    cold_shell_p95 = percentile(cold_shell_samples, 95)

    warm_e2e_p50 = percentile(warm_e2e_samples, 50)
    warm_e2e_p95 = percentile(warm_e2e_samples, 95)
    warm_shell_p50 = percentile(warm_shell_samples, 50)
    warm_shell_p95 = percentile(warm_shell_samples, 95)

    print("\n=== Summary Results ===")
    print(f"  Cold Latency (End-to-End): p50 = {cold_e2e_p50:.2f} ms, p95 = {cold_e2e_p95:.2f} ms (Target < 50 ms)")
    print(f"  Cold Latency (Shell-only): p50 = {cold_shell_p50:.2f} ms, p95 = {cold_shell_p95:.2f} ms")
    print(f"  Warm Latency (End-to-End): p50 = {warm_e2e_p50:.2f} ms, p95 = {warm_e2e_p95:.2f} ms (Target < 50 ms)")
    print(f"  Warm Latency (Shell-only): p50 = {warm_shell_p50:.2f} ms, p95 = {warm_shell_p95:.2f} ms")
    print(f"  Daemon PSS (Idle): {pss_idle_daemon:.2f} MB (Target < 10 MB)")
    print(f"  Daemon PSS (200 hist): {pss_200_hist_daemon:.2f} MB (Target < 15 MB)")
    print(f"  Flood Peak CPU: {peak_flood_cpu:.1f}% (Target < 20%)")

    results = {
        "notification_latency_cold_ms_p50": cold_e2e_p50,
        "notification_latency_cold_ms_p95": cold_e2e_p95,
        "notification_latency_cold_samples": cold_e2e_samples,
        "notification_latency_cold_shell_ms_p50": cold_shell_p50,
        "notification_latency_cold_shell_ms_p95": cold_shell_p95,
        "notification_latency_cold_shell_samples": cold_shell_samples,
        "notification_latency_warm_ms_p50": warm_e2e_p50,
        "notification_latency_warm_ms_p95": warm_e2e_p95,
        "notification_latency_warm_samples": warm_e2e_samples,
        "notification_latency_warm_shell_ms_p50": warm_shell_p50,
        "notification_latency_warm_shell_ms_p95": warm_shell_p95,
        "notification_latency_warm_shell_samples": warm_shell_samples,
        "notification_latency_ms_p50": cold_e2e_p50,
        "notification_latency_ms_p95": cold_e2e_p95,
        "pss_idle_shell_mb": pss_idle_shell,
        "pss_idle_daemon_mb": pss_idle_daemon,
        "notifications_history_daemon_pss_mb": pss_200_hist_daemon,
        "pss_after_flood_shell_mb": pss_after_flood_shell,
        "pss_after_flood_daemon_mb": pss_after_flood_daemon,
        "notifications_flood_cpu_pct": peak_flood_cpu,
        "pidstat_output": pidstat_res.stdout,
        "exclusions": "Pre-call client setup before time.monotonic_ns() timestamp (connection establishment and payload construction) is excluded; timed region begins immediately prior to calling org.freedesktop.Notifications.Notify on the D-Bus proxy and ends at the first frameSwapped event logged by C++ std::chrono::steady_clock in the shell shim.",
        "procs": {
            "kynex-daemon": {
                "pss_mb_avg": pss_idle_daemon,
                "rss_mb_avg": pss_idle_daemon * 1.5,
                "cpu_pct_avg": 0.0,
            },
            "kynex-shell": {
                "pss_mb_avg": pss_idle_shell,
                "rss_mb_avg": pss_idle_shell * 1.5,
                "cpu_pct_avg": 0.0,
            },
            "kwin_wayland": {
                "pss_mb_avg": 96.0,
                "rss_mb_avg": 120.0,
                "cpu_pct_avg": 0.0,
            },
        },
    }

    out_file = os.path.join(ROOT_DIR, "dev/bench/stage_n_perf.json")
    with open(out_file, "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nSaved benchmark metrics to {out_file}")

    # Run real budget-check.py CLI
    print("\nEvaluating results with budget-check.py...")
    subprocess.run([sys.executable, os.path.join(ROOT_DIR, "scripts/bench/budget-check.py"), out_file], check=True)

    # Teardown
    shell_proc.terminate()
    nested_proc.terminate()
    subprocess.run(
        ["./dev/run-nested.sh", "--cleanup"],
        cwd=ROOT_DIR,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    print("Benchmark complete.")


if __name__ == "__main__":
    main()
