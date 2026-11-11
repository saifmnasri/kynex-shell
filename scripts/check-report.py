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

    report_name = os.path.basename(report_path)
    file_size = os.path.getsize(report_path)
    if ("o2" in report_name or "stage-o2" in report_name) and file_size > 150 * 1024:
        print(
            f"ERROR: Report {report_name} exceeds 150 KB limit ({file_size / 1024:.1f} KB)!",
            file=sys.stderr,
        )
        return False

    with open(report_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Match code blocks starting with ```raw:<id> or ```raw:<id>:L<a>-L<b>
    pattern = re.compile(
        r"^(`{3,})raw:([a-zA-Z0-9_\.\-]+)(?::L(\d+)-L(\d+))?[^\n]*\n([\s\S]*?)\n\1",
        re.MULTILINE,
    )
    matches = list(pattern.finditer(content))

    print(f"Checking evidence blocks in {report_name} ({file_size / 1024:.1f} KB)...")

    if not matches:
        # If this is stage-n2-report.md or later, require at least one raw block
        if any(tok in report_name for tok in ("n2", "stage-o", "stage-o2")):
            print(f"ERROR: No ```raw:<id> blocks found in {report_name}!", file=sys.stderr)
            return False
        print(f"Notice: No ```raw:<id> blocks found in legacy report {report_name} (skipped).")
        return True

    failed = False
    checked_count = 0

    for m in matches:
        evidence_id = m.group(2)
        start_str = m.group(3)
        end_str = m.group(4)
        block_text = m.group(5)
        evidence_file = os.path.join(EVIDENCE_DIR, f"{evidence_id}.txt")

        if not os.path.isfile(evidence_file):
            print(f"  FAILED: Missing evidence file: {evidence_file}", file=sys.stderr)
            failed = True
            continue

        with open(evidence_file, "r", encoding="utf-8") as ef:
            evidence_text = ef.read()

        if start_str is not None and end_str is not None:
            start_line = int(start_str)
            end_line = int(end_str)
            num_lines = end_line - start_line + 1

            if num_lines > 60:
                print(
                    f"  FAILED: Block ```raw:{evidence_id}:L{start_line}-L{end_line} exceeds 60 lines ({num_lines} lines)!",
                    file=sys.stderr,
                )
                failed = True
                continue

            if start_line < 1 or end_line < start_line:
                print(
                    f"  FAILED: Invalid line range L{start_line}-L{end_line} for {evidence_id}!",
                    file=sys.stderr,
                )
                failed = True
                continue

            evidence_lines = evidence_text.splitlines()
            if end_line > len(evidence_lines):
                print(
                    f"  FAILED: Line range L{start_line}-L{end_line} exceeds evidence file length ({len(evidence_lines)} lines)!",
                    file=sys.stderr,
                )
                failed = True
                continue

            expected_lines = evidence_lines[start_line - 1 : end_line]
            block_lines = block_text.splitlines()

            if block_lines != expected_lines:
                print(
                    f"  FAILED: Block ```raw:{evidence_id}:L{start_line}-L{end_line} does not match {evidence_file} lines {start_line}-{end_line} verbatim!",
                    file=sys.stderr,
                )
                failed = True
            else:
                checked_count += 1
                print(
                    f"  ✓ ```raw:{evidence_id}:L{start_line}-L{end_line} verified verbatim ({num_lines} lines)"
                )
        else:
            if "o2" in report_name or "stage-o2" in report_name:
                print(
                    f"  FAILED: Block ```raw:{evidence_id} in {report_name} must specify explicit line range :L<a>-L<b> (<= 60 lines)!",
                    file=sys.stderr,
                )
                failed = True
                continue

            # Legacy subslice verification
            block_lines = [line.rstrip() for line in block_text.splitlines()]
            evidence_lines = [line.rstrip() for line in evidence_text.splitlines()]

            while block_lines and block_lines[-1] == "":
                block_lines.pop()
            while block_lines and block_lines[0] == "":
                block_lines.pop(0)

            while evidence_lines and evidence_lines[-1] == "":
                evidence_lines.pop()

            if not is_contiguous_subslice(block_lines, evidence_lines):
                print(
                    f"  FAILED: Block ```raw:{evidence_id} in {report_name} does not match {evidence_file} verbatim!",
                    file=sys.stderr,
                )
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

