#!/usr/bin/env python3
"""Measure memory (PSS/RSS), CPU and thread count of named processes over time.
Use the SAME machine/VM settings for Plasma and Kynex baselines.
Example: scripts/bench/measure.py --label kynex-idle --procs kwin_wayland kynex-shell kynex-daemon --duration 120
         scripts/bench/measure.py --label plasma-idle --procs kwin_wayland plasmashell --duration 120
Output: table + JSON in dev/bench/. Methodology: docs/PERFORMANCE.md
"""
import argparse, json, os, time, datetime, subprocess, sys

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
    try:
        with open(f"/proc/{pid}/smaps_rollup") as f:
            for line in f:
                if line.startswith("Pss:"): pss = int(line.split()[1])
                elif line.startswith("Rss:"): rss = int(line.split()[1])
    except PermissionError:
        try:
            with open(f"/proc/{pid}/status") as f:
                vm_rss = anon = file = 0
                for line in f:
                    if line.startswith("VmRSS:"): vm_rss = int(line.split()[1])
                    elif line.startswith("RssAnon:"): anon = int(line.split()[1])
                    elif line.startswith("RssFile:"): file = int(line.split()[1])
                rss = vm_rss
                pss = (anon + file // 2) if (anon or file) else rss
        except OSError:
            pass
    return pss, rss

def threads(pid):
    return len(os.listdir(f"/proc/{pid}/task"))

def get_git_sha():
    try:
        return subprocess.check_output(["git", "rev-parse", "--short=8", "HEAD"], text=True).strip()
    except Exception:
        return "unknown"

def detect_build_profile(procs):
    for name in procs:
        for pid in pids(name):
            try:
                exe = os.readlink(f"/proc/{pid}/exe")
                if "/release/" in exe:
                    return "release"
                elif "/debug/" in exe:
                    return "debug"
            except OSError:
                pass
    if os.path.exists("target/release/kynex-shell"):
        return "release"
    return "debug"

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", required=True)
    ap.add_argument("--procs", nargs="+", required=True)
    ap.add_argument("--duration", type=int, default=120)
    ap.add_argument("--interval", type=float, default=5.0)
    ap.add_argument("--build-profile", choices=["release", "debug"], help="Build profile (default: auto-detected)")
    ap.add_argument("--allow-multiple", action="store_true", help="Allow multiple instances of measured processes")
    a = ap.parse_args()

    for name in a.procs:
        found_pids = pids(name)
        if len(found_pids) > 1 and not a.allow_multiple:
            sys.exit(f"ERROR: Multiple instances of '{name}' running: {found_pids}. Pass --allow-multiple to allow.")

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
            tot_pss = tot_rss = tot_threads = 0
            for pid in pids(name):
                try:
                    pss, rss = read_mem(pid); tot_pss += pss; tot_rss += rss; tot_threads += threads(pid)
                except OSError: pass
            samples[name].append((tot_pss, tot_rss, tot_threads))
    elapsed = time.time() - t0
    git_sha = get_git_sha()
    build_profile = a.build_profile or detect_build_profile(a.procs)
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    result = {
        "label": a.label,
        "date": datetime.datetime.now().isoformat(),
        "timestamp": timestamp,
        "git_sha": git_sha,
        "build_profile": build_profile,
        "duration_s": round(elapsed, 1),
        "procs": {},
    }
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
    filename = f"{a.label}-{timestamp}-{git_sha}.json"
    path = f"dev/bench/{filename}"
    with open(path, "w") as f: json.dump(result, f, indent=2)
    print(f"\nSaved {path}")

if __name__ == "__main__":
    main()
