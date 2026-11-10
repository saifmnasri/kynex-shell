#!/usr/bin/env python3
"""Check that ```raw:<id> blocks in work reports match docs/work/evidence/<id>.txt verbatim."""

import glob
import os
import re
import sys

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
EVIDENCE_DIR = os.path.join(ROOT_DIR, "docs/work/evidence")


def find_newest_report():
    reports = glob.glob(os.path.join(ROOT_DIR, "docs/work/*-report.md"))
    if not reports:
        return None
    # Sort by mtime descending
    reports.sort(key=lambda p: os.path.getmtime(p), reverse=True)
    return reports[0]


def is_contiguous_subslice(sub, full):
    if not sub:
        return True
    n = len(sub)
    if n > len(full):
        return False
    for i in range(len(full) - n + 1):
        if full[i : i + n] == sub:
            return True
    return False


def verify_report(report_path):
    if not os.path.isfile(report_path):
        print(f"ERROR: Report file not found: {report_path}", file=sys.stderr)
        return False

    with open(report_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Match code blocks starting with ```raw:<id> or ````raw:<id>
    pattern = re.compile(r"^(`{3,})raw:([a-zA-Z0-9_\.\-]+)[^\n]*\n([\s\S]*?)\n\1", re.MULTILINE)
    matches = list(pattern.finditer(content))

    report_name = os.path.basename(report_path)
    print(f"Checking evidence blocks in {report_name}...")

    if not matches:
        # If this is stage-n2-report.md or later, require at least one raw block
        if "n2" in report_name or "stage-n2" in report_name:
            print(f"ERROR: No ```raw:<id> blocks found in {report_name}!", file=sys.stderr)
            return False
        print(f"Notice: No ```raw:<id> blocks found in legacy report {report_name} (skipped).")
        return True

    failed = False
    checked_count = 0

    for m in matches:
        evidence_id = m.group(2)
        block_text = m.group(3)
        evidence_file = os.path.join(EVIDENCE_DIR, f"{evidence_id}.txt")

        if not os.path.isfile(evidence_file):
            print(f"  FAILED: Missing evidence file: {evidence_file}", file=sys.stderr)
            failed = True
            continue

        with open(evidence_file, "r", encoding="utf-8") as ef:
            evidence_text = ef.read()

        block_lines = [line.rstrip() for line in block_text.splitlines()]
        evidence_lines = [line.rstrip() for line in evidence_text.splitlines()]

        while block_lines and block_lines[-1] == "":
            block_lines.pop()
        while block_lines and block_lines[0] == "":
            block_lines.pop(0)

        while evidence_lines and evidence_lines[-1] == "":
            evidence_lines.pop()

        if not is_contiguous_subslice(block_lines, evidence_lines):
            print(f"  FAILED: Block ```raw:{evidence_id} in {report_name} does not match {evidence_file} verbatim!", file=sys.stderr)
            print("  --- Block lines (first 5) ---", file=sys.stderr)
            for l in block_lines[:5]:
                print(f"    {l}", file=sys.stderr)
            print("  --- Evidence lines (first 5) ---", file=sys.stderr)
            for l in evidence_lines[:5]:
                print(f"    {l}", file=sys.stderr)
            failed = True
        else:
            checked_count += 1
            print(f"  ✓ ```raw:{evidence_id} verified against evidence/{evidence_id}.txt")

    if failed:
        print(f"ERROR: Evidence verification failed for {report_name}.", file=sys.stderr)
        return False

    print(f"SUCCESS: All {checked_count} raw evidence blocks in {report_name} match evidence files verbatim.")
    return True


def main():
    if len(sys.argv) > 1:
        target = sys.argv[1]
    else:
        target = find_newest_report()
        if not target:
            print("No report found in docs/work.", file=sys.stderr)
            sys.exit(1)

    success = verify_report(target)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()

