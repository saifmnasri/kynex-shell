#!/usr/bin/env python3
"""Check that ```raw:<id> blocks in work reports match docs/work/evidence/<id>.txt verbatim."""

import glob
import os
import re
import subprocess
import sys
import tomllib

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
EVIDENCE_DIR = os.path.join(ROOT_DIR, "docs/work/evidence")
CLAIMS_DIR = os.path.join(ROOT_DIR, "docs/work/claims")


def find_newest_report():
    reports = glob.glob(os.path.join(ROOT_DIR, "docs/work/*-report.md"))
    if not reports:
        return None
    # Prioritize active stage reports
    for stage in ("stage-p3", "stage-p2"):
        p = os.path.join(ROOT_DIR, f"docs/work/{stage}-report.md")
        if os.path.isfile(p):
            return p
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


def verify_claims(report_path, content, report_name):
    stage_name = re.sub(r"-report\.md$", "", report_name)
    claims_file = os.path.join(CLAIMS_DIR, f"{stage_name}.toml")

    # If stage-p2 or claims file exists, require claims verification
    if "p2" in report_name or "stage-p2" in report_name or os.path.isfile(claims_file):
        if not os.path.isfile(claims_file):
            print(f"ERROR: Missing claims ledger file: {claims_file}", file=sys.stderr)
            return False

        print(f"Checking claims ledger in {os.path.basename(claims_file)}...")
        with open(claims_file, "rb") as cf:
            raw_claims = tomllib.load(cf)

        claims_dict = raw_claims.get("claims", raw_claims) if isinstance(raw_claims.get("claims"), dict) else raw_claims

        # Extract Executive Summary section
        exec_match = re.search(
            r"^#+\s+(?:\d+\.\s*)?Executive Summary[^\n]*\n([\s\S]*?)(?=^#+\s+|\Z)",
            content,
            re.MULTILINE | re.IGNORECASE,
        )
        if not exec_match:
            print(f"ERROR: Executive Summary section not found in {report_name}!", file=sys.stderr)
            return False

        exec_text = exec_match.group(1)
        bullet_pattern = re.compile(r"^\s*(?:[-*]|\d+\.)\s+(.+)$", re.MULTILINE)
        bullets = bullet_pattern.findall(exec_text)

        if not bullets:
            print(f"ERROR: No bullet items found in Executive Summary of {report_name}!", file=sys.stderr)
            return False

        claims_referenced = []
        failed_claims = False

        for b in bullets:
            b_clean = b.strip()
            claim_m = re.search(r"\{claim:([a-zA-Z0-9_\-]+)\}\s*$", b_clean)
            if not claim_m:
                print(f"  FAILED: Executive summary bullet does not end with {{claim:<id>}}:\n    '{b_clean}'", file=sys.stderr)
                failed_claims = True
                continue
            cid = claim_m.group(1)
            claims_referenced.append(cid)

        for cid in claims_referenced:
            if cid not in claims_dict:
                print(f"  FAILED: Claim '{cid}' referenced in executive summary is not defined in {claims_file}!", file=sys.stderr)
                failed_claims = True

        for cid in claims_dict.keys():
            if cid not in claims_referenced:
                print(f"  FAILED: Claim '{cid}' defined in {claims_file} is not referenced in executive summary!", file=sys.stderr)
                failed_claims = True

        for cid, entry in claims_dict.items():
            if not isinstance(entry, dict):
                print(f"  FAILED: Claim '{cid}' entry must be a dictionary!", file=sys.stderr)
                failed_claims = True
                continue

            if "untested" in entry:
                reason = entry["untested"]
                print(f"  [UNTESTED] claim '{cid}': {reason}")
                continue

            test_path = entry.get("test_path")
            test_name = entry.get("test_name")
            evidence_id = entry.get("evidence_id")

            if not (test_path and test_name and evidence_id):
                print(
                    f"  FAILED: Claim '{cid}' must specify 'test_path', 'test_name', and 'evidence_id' (or 'untested')!",
                    file=sys.stderr,
                )
                failed_claims = True
                continue

            abs_test_path = os.path.join(ROOT_DIR, test_path)
            if not os.path.isfile(abs_test_path):
                print(f"  FAILED: Test path '{test_path}' does not exist for claim '{cid}'!", file=sys.stderr)
                failed_claims = True
                continue

            try:
                with open(abs_test_path, "r", encoding="utf-8") as tf:
                    test_file_content = tf.read()
            except Exception as ex:
                print(f"  FAILED: Cannot read test file '{test_path}': {ex}", file=sys.stderr)
                failed_claims = True
                continue

            if test_name not in test_file_content:
                print(f"  FAILED: Test name '{test_name}' not found in repo at '{test_path}' for claim '{cid}'!", file=sys.stderr)
                failed_claims = True
                continue

            ev_path = os.path.join(EVIDENCE_DIR, f"{evidence_id}.txt")
            if not os.path.isfile(ev_path):
                print(f"  FAILED: Evidence file '{ev_path}' does not exist for claim '{cid}'!", file=sys.stderr)
                failed_claims = True
                continue

            try:
                with open(ev_path, "r", encoding="utf-8") as ef:
                    ev_lines = ef.readlines()
            except Exception as ex:
                print(f"  FAILED: Cannot read evidence file '{ev_path}': {ex}", file=sys.stderr)
                failed_claims = True
                continue

            pass_found = False
            for line in ev_lines:
                if test_name in line:
                    line_s = line.strip()
                    if re.search(r"\b(?:ok|OK|PASSED|PASS)\b", line_s) and not re.search(r"\b(?:FAILED|FAIL|panicked)\b", line_s):
                        pass_found = True
                        break

            if not pass_found:
                print(
                    f"  FAILED: Evidence file '{evidence_id}.txt' does not contain passing line for test '{test_name}' (claim '{cid}')!",
                    file=sys.stderr,
                )
                failed_claims = True
            else:
                print(f"  ✓ Claim '{cid}' verified: {test_path} :: {test_name} in evidence/{evidence_id}.txt")

        if failed_claims:
            return False

        print(f"SUCCESS: All {len(claims_dict)} claims in {os.path.basename(claims_file)} verified.")
    return True


def verify_guard_changes(report_path, content, report_name):
    # Only enforce for stage-p3 or reports containing GUARD CHANGES
    guard_match = re.search(
        r"^#+\s+(?:\d+\.\s*)?GUARD CHANGES[^\n]*\n([\s\S]*?)(?=^#+\s+|\Z)",
        content,
        re.MULTILINE | re.IGNORECASE,
    )

    is_p3 = any(tok in report_name for tok in ("p3", "stage-p3"))
    if not guard_match:
        if is_p3:
            print(f"ERROR: GUARD CHANGES section not found in {report_name}!", file=sys.stderr)
            return False
        return True

    guard_text = guard_match.group(1)
    bullet_pattern = re.compile(r"^\s*(?:[-*]|\d+\.)\s+(.+)$", re.MULTILINE)
    bullets = bullet_pattern.findall(guard_text)

    files_in_report = set()
    for b in bullets:
        m = re.search(
            r"^\s*`?([a-zA-Z0-9_\-\./]+(?:\.[a-zA-Z0-9]+|\.toml|\.txt|\.sh|\.py|\.md|\.yaml|\.json))`?\s*[:\-]",
            b,
        )
        if m:
            files_in_report.add(m.group(1).strip())
        else:
            backticked = re.findall(r"`([^`]+)`", b)
            if backticked:
                files_in_report.add(backticked[0].strip())

    guard_script = os.path.join(ROOT_DIR, "scripts/guard-diff.sh")
    if not os.path.isfile(guard_script):
        print(f"ERROR: scripts/guard-diff.sh not found for guard verification!", file=sys.stderr)
        return False

    sub_env = os.environ.copy()
    sub_env["GIT_CONFIG_COUNT"] = "1"
    sub_env["GIT_CONFIG_KEY_0"] = "safe.directory"
    sub_env["GIT_CONFIG_VALUE_0"] = "*"
    sub_env["GIT_DIR"] = os.path.join(ROOT_DIR, ".git")
    sub_env["GIT_WORK_TREE"] = ROOT_DIR

    res = subprocess.run([guard_script, "--list"], cwd=ROOT_DIR, env=sub_env, capture_output=True, text=True)
    if res.returncode != 0:
        print(f"ERROR: scripts/guard-diff.sh --list failed with code {res.returncode}:\n{res.stderr}", file=sys.stderr)
        return False

    expected_files = set(line.strip() for line in res.stdout.splitlines() if line.strip())

    print(f"Checking GUARD CHANGES in {report_name} against scripts/guard-diff.sh...")
    missing_in_report = expected_files - files_in_report
    extra_in_report = files_in_report - expected_files

    if missing_in_report:
        print(f"ERROR: Changed guard files missing from GUARD CHANGES in {report_name}:", file=sys.stderr)
        for f in sorted(missing_in_report):
            print(f"  - {f}", file=sys.stderr)
        return False

    if extra_in_report:
        print(f"ERROR: Files listed in GUARD CHANGES were not reported by scripts/guard-diff.sh:", file=sys.stderr)
        for f in sorted(extra_in_report):
            print(f"  - {f}", file=sys.stderr)
        return False

    print(f"SUCCESS: All {len(expected_files)} guard changes in {report_name} match scripts/guard-diff.sh exactly.")
    return True


def verify_report(report_path):
    if not os.path.isfile(report_path):
        print(f"ERROR: Report file not found: {report_path}", file=sys.stderr)
        return False

    report_name = os.path.basename(report_path)
    file_size = os.path.getsize(report_path)
    if any(tok in report_name for tok in ("o2", "stage-o2", "p2", "stage-p2", "p3", "stage-p3")) and file_size > 150 * 1024:
        print(
            f"ERROR: Report {report_name} exceeds 150 KB limit ({file_size / 1024:.1f} KB)!",
            file=sys.stderr,
        )
        return False

    with open(report_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Claims ledger verification
    if not verify_claims(report_path, content, report_name):
        return False

    # Guard changes verification
    if not verify_guard_changes(report_path, content, report_name):
        return False

    # Match code blocks starting with ```raw:<id> or ```raw:<id>:L<a>-L<b>
    pattern = re.compile(
        r"^(`{3,})raw:([a-zA-Z0-9_\.\-]+)(?::L(\d+)-L(\d+))?[^\n]*\n([\s\S]*?)\n\1",
        re.MULTILINE,
    )
    matches = list(pattern.finditer(content))

    print(f"Checking evidence blocks in {report_name} ({file_size / 1024:.1f} KB)...")

    if not matches:
        # If this is stage-n2-report.md or later, require at least one raw block
        if any(tok in report_name for tok in ("n2", "stage-o", "stage-o2", "p2", "stage-p2", "p3", "stage-p3")):
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
            if any(tok in report_name for tok in ("o2", "stage-o2", "p2", "stage-p2", "p3", "stage-p3")):
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

