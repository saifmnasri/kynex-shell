#!/usr/bin/env python3
"""Measure memory (PSS/RSS), CPU and thread count of named processes over time.
Use the SAME machine/VM settings for Plasma and Kynex baselines.
Example: scripts/bench/measure.py --label kynex-idle --procs kwin_wayland kynex-shell kynex-daemon --duration 120
         scripts/bench/measure.py --label plasma-idle --procs kwin_wayland plasmashell --duration 120
Output: table + JSON in dev/bench/. Methodology: docs/PERFORMANCE.md
"""
import argparse, json, os, time, datetime, subprocess

CLK = os.sysconf(os.sysconf_names["SC_CLK_TCK"])

def pids(name):
    out = subprocess.run(["pgrep", "-x", name], capture_output=True, text=True).stdout.split()
    return [int(p) for p in out]

def read_cpu(pid):
    with open(f"/proc/{pid}/stat") as f:
        parts = f.read().rsplit(")", 1)[1].split()
    return (int(parts[11]) + int(parts[12])) / CLK  # utime + stime in seconds

def read_mem(pid):
    pss = rss = 0
    with open(f"/proc/{pid}/smaps_rollup") as f:
        for line in f:
            if line.startswith("Pss:"): pss = int(line.split()[1])
            elif line.startswith("Rss:"): rss = int(line.split()[1])
    return pss, rss

def threads(pid):
    return len(os.listdir(f"/proc/{pid}/task"))

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", required=True)
    ap.add_argument("--procs", nargs="+", required=True)
    ap.add_argument("--duration", type=int, default=120)
    ap.add_argument("--interval", type=float, default=5.0)
    a = ap.parse_args()

    samples = {p: [] for p in a.procs}
    cpu0, t0 = {}, time.time()
    for name in a.procs:
        for pid in pids(name):
            try: cpu0[(name, pid)] = read_cpu(pid)
            except OSError: pass
    end = t0 + a.duration
    while time.time() < end:
        time.sleep(a.interval)
        for name in a.procs:
            tot_pss = tot_rss = tot_thr = 0
            for pid in pids(name):
                try:
                    pss, rss = read_mem(pid); tot_pss += pss; tot_rss += rss; tot_thr += threads(pid)
                except OSError: pass
            samples[name].append((tot_pss, tot_rss, tot_thr))
    elapsed = time.time() - t0
    result = {"label": a.label, "date": datetime.datetime.now().isoformat(), "duration_s": round(elapsed, 1), "procs": {}}
    print(f"\n{'process':<16}{'PSS MB (avg)':>14}{'PSS MB (max)':>14}{'RSS MB (avg)':>14}{'threads':>9}{'CPU % (avg)':>13}")
    for name in a.procs:
        s = samples[name]
        if not s or all(x[0] == 0 for x in s):
            print(f"{name:<16}{'not running':>14}"); result["procs"][name] = None; continue
        cpu_used = 0.0
        for (n, pid), c0 in cpu0.items():
            if n == name:
                try: cpu_used += read_cpu(pid) - c0
                except OSError: pass
        avg = lambda i: sum(x[i] for x in s) / len(s)
        row = {"pss_mb_avg": round(avg(0)/1024, 1), "pss_mb_max": round(max(x[0] for x in s)/1024, 1),
               "rss_mb_avg": round(avg(1)/1024, 1), "threads": round(avg(2)), "cpu_pct_avg": round(100*cpu_used/elapsed, 2)}
        result["procs"][name] = row
        print(f"{name:<16}{row['pss_mb_avg']:>14}{row['pss_mb_max']:>14}{row['rss_mb_avg']:>14}{row['threads']:>9}{row['cpu_pct_avg']:>13}")
    os.makedirs("dev/bench", exist_ok=True)
    path = f"dev/bench/{datetime.date.today()}-{a.label}.json"
    with open(path, "w") as f: json.dump(result, f, indent=2)
    print(f"\nSaved {path}")

if __name__ == "__main__":
    main()
