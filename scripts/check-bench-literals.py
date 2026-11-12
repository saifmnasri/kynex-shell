#!/usr/bin/env python3
"""AST-based numeric literal checker for benchmark scripts.

Enforces zero hardcoded metric values by flagging every numeric literal (and string-to-number
conversion) in dev/bench/*.py and scripts/bench/*.py unless explicitly listed with a
one-line justification in scripts/bench/literals-allowlist.txt.
"""

import ast
import glob
import os
import sys

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ALLOWLIST_PATH = os.path.join(ROOT_DIR, "scripts/bench/literals-allowlist.txt")


def load_allowlist(allowlist_path):
    allowed = set()
    if not os.path.isfile(allowlist_path):
        return allowed
    with open(allowlist_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split("|")
            if len(parts) >= 3:
                file_rel = os.path.normpath(parts[0].strip())
                line_text = "|".join(parts[1:-1]).strip()
                allowed.add((file_rel, line_text))
            elif len(parts) == 2:
                file_rel = os.path.normpath(parts[0].strip())
                line_text = parts[1].strip()
                allowed.add((file_rel, line_text))
    return allowed


def find_numeric_literals(fpath, source_lines):
    tree = ast.parse("".join(source_lines), filename=fpath)
    hits = []
    for node in ast.walk(tree):
        hit = False
        val_desc = ""
        if (
            isinstance(node, ast.Constant)
            and isinstance(node.value, (int, float, complex))
            and not isinstance(node.value, bool)
        ):
            hit = True
            val_desc = repr(node.value)
        elif (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and node.func.id in ("float", "int")
            and node.args
            and isinstance(node.args[0], ast.Constant)
            and isinstance(node.args[0].value, str)
        ):
            try:
                float(node.args[0].value)
                hit = True
                val_desc = f"{node.func.id}({node.args[0].value!r})"
            except ValueError:
                pass

        if hit and hasattr(node, "lineno"):
            lineno = node.lineno
            line_text = source_lines[lineno - 1].strip()
            hits.append((lineno, line_text, val_desc))
    return hits


def check_files(files, allowlist):
    violations = []
    for fpath in files:
        rel_path = os.path.normpath(os.path.relpath(fpath, ROOT_DIR))
        with open(fpath, "r", encoding="utf-8") as f:
            lines = f.readlines()
        hits = find_numeric_literals(fpath, lines)
        seen_lines = set()
        for lineno, line_text, val_desc in hits:
            if lineno in seen_lines:
                continue
            seen_lines.add(lineno)
            if (rel_path, line_text) not in allowlist:
                violations.append((rel_path, lineno, line_text, val_desc))
    return violations


def main():
    allowlist = load_allowlist(ALLOWLIST_PATH)
    if len(sys.argv) > 1:
        target_files = [os.path.abspath(p) for p in sys.argv[1:]]
    else:
        pattern1 = os.path.join(ROOT_DIR, "dev/bench/*.py")
        pattern2 = os.path.join(ROOT_DIR, "scripts/bench/*.py")
        target_files = sorted(glob.glob(pattern1) + glob.glob(pattern2))

    violations = check_files(target_files, allowlist)
    if violations:
        print("ERROR: Unallowlisted numeric literal(s) detected in benchmark scripts:", file=sys.stderr)
        for rel_path, lineno, line_text, val_desc in violations:
            print(f"  {rel_path}:{lineno} [{val_desc}]: {line_text}", file=sys.stderr)
        print(f"\nTotal violations: {len(violations)}", file=sys.stderr)
        print("Every numeric literal in dev/bench/ and scripts/bench/ must be listed with a justification in scripts/bench/literals-allowlist.txt.", file=sys.stderr)
        sys.exit(1)

    print(f"PASS: All numeric literals in {len(target_files)} benchmark files match allowlist ({len(allowlist)} allowed lines).")
    sys.exit(0)


if __name__ == "__main__":
    main()
