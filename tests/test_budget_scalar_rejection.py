#!/usr/bin/env python3
"""Test that scripts/bench/budget-check.py rejects scalar-only metrics and accepts sampled metrics."""

import json
import os
import subprocess
import sys
import tempfile

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
BUDGET_CHECK_PY = os.path.join(ROOT_DIR, "scripts/bench/budget-check.py")
BUDGETS_TOML = os.path.join(ROOT_DIR, "scripts/bench/budgets.toml")


def test_scalar_rejection():
    # 1. Hand-typed scalar metric: must be rejected with exit code 1
    scalar_payload = {
        "tray_flood_cpu_pct": 4.8,
        "procs": {
            "kynex-daemon": {"pss_mb_avg": 8.0, "rss_mb_avg": 12.0, "cpu_pct_avg": 0.0},
            "kynex-shell": {"pss_mb_avg": 20.0, "rss_mb_avg": 30.0, "cpu_pct_avg": 0.0},
            "kwin_wayland": {"pss_mb_avg": 50.0, "rss_mb_avg": 60.0, "cpu_pct_avg": 0.0},
        },
    }

    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as tf:
        json.dump(scalar_payload, tf)
        scalar_path = tf.name

    try:
        proc = subprocess.run(
            [sys.executable, BUDGET_CHECK_PY, scalar_path, "--budgets", BUDGETS_TOML],
            capture_output=True,
            text=True,
        )
        assert proc.returncode != 0, "budget-check.py must REJECT scalar-only metric!"
        assert (
            "Scalar-only metrics are rejected" in proc.stderr
            or "is a scalar" in proc.stderr
        ), f"Unexpected stderr: {proc.stderr}"
        print("✓ Hand-typed scalar metric rejected as expected.")
    finally:
        os.remove(scalar_path)

    # 2. Structured metric with raw samples array and method: must PASS
    valid_payload = {
        "tray_flood_cpu": {
            "method": "/proc/<pid>/stat delta sampling",
            "samples": [2.5, 3.1, 4.2, 3.8, 2.9],
        },
        "tray_item_registration_cold": {
            "method": "clock_gettime monotonic e2e",
            "samples": [10.5, 9.8, 11.2, 10.1, 10.9],
        },
        "tray_item_registration_warm": {
            "method": "clock_gettime monotonic warm e2e",
            "samples": [7.8, 7.5, 8.0, 7.6, 7.9],
        },
        "tray_idle_daemon_pss": {
            "method": "/proc/smaps_rollup PSS plateau",
            "samples": [7.85, 7.86, 7.85, 7.87, 7.86],
        },
        "procs": {
            "kynex-daemon": {"pss_mb_avg": 7.86, "rss_mb_avg": 12.0, "cpu_pct_avg": 0.0},
            "kynex-shell": {"pss_mb_avg": 20.0, "rss_mb_avg": 30.0, "cpu_pct_avg": 0.0},
            "kwin_wayland": {"pss_mb_avg": 50.0, "rss_mb_avg": 60.0, "cpu_pct_avg": 0.0},
        },
    }

    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as tf:
        json.dump(valid_payload, tf)
        valid_path = tf.name

    try:
        proc = subprocess.run(
            [sys.executable, BUDGET_CHECK_PY, valid_path, "--budgets", BUDGETS_TOML],
            capture_output=True,
            text=True,
        )
        assert proc.returncode == 0, f"budget-check.py failed on valid sampled metric: {proc.stderr}\n{proc.stdout}"
        print("✓ Structured metric with raw samples and method accepted and verified.")
    finally:
        os.remove(valid_path)


if __name__ == "__main__":
    test_scalar_rejection()
    print("All budget-check tests passed!")

