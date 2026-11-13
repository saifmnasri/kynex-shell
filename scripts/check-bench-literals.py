#!/usr/bin/env python3
"""AST-based numeric literal checker and benchmark integrity enforcement.

Enforces:
1. Zero hardcoded metric values in benchmark collectors and analyzers.
2. Strict prohibition of measurement data manipulation: no numeric literal may appear
   in a BinOp or AugAssign together with any variable derived from measurements or samples.
   (This rule is unconditional and cannot be bypassed via allowlist).
3. Every permitted numeric literal (CLI defaults, unit constants, loop bounds) must be
   listed with a one-line justification in scripts/bench/literals-allowlist.txt.
"""

import ast
import os
import sys

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ALLOWLIST_PATH = os.path.join(ROOT_DIR, "scripts/bench/literals-allowlist.txt")

MEASUREMENT_NAME_PATTERNS = (
    "sample",
    "measur",
    "lat",
    "e2e",
    "elapsed",
    "pss",
    "rss",
    "cpu",
    "frame",
    "timestamp",
    "shell_lat",
)


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


def is_numeric_literal_node(node):
    if (
        isinstance(node, ast.Constant)
        and isinstance(node.value, (int, float, complex))
        and not isinstance(node.value, bool)
    ):
        return True, repr(node.value)
    if (
        isinstance(node, ast.Call)
        and isinstance(node.func, ast.Name)
        and node.func.id in ("float", "int")
        and node.args
        and isinstance(node.args[0], ast.Constant)
        and isinstance(node.args[0].value, str)
    ):
        try:
            float(node.args[0].value)
            return True, f"{node.func.id}({node.args[0].value!r})"
        except ValueError:
            pass
    return False, ""


def find_measurement_name(node):
    for child in ast.walk(node):
        if isinstance(child, ast.Name):
            name_lower = child.id.lower()
            if any(p in name_lower for p in MEASUREMENT_NAME_PATTERNS):
                return child.id
    return None


def find_forbidden_measurement_arithmetic(fpath, source_lines):
    tree = ast.parse("".join(source_lines), filename=fpath)
    violations = []
    for node in ast.walk(tree):
        hit = False
        reason = ""
        if isinstance(node, ast.BinOp):
            left_lit, left_desc = is_numeric_literal_node(node.left)
            right_lit, right_desc = is_numeric_literal_node(node.right)
            if left_lit:
                m_name = find_measurement_name(node.right)
                if m_name:
                    hit = True
                    reason = f"Numeric literal {left_desc} in BinOp with measurement variable '{m_name}'"
            elif right_lit:
                m_name = find_measurement_name(node.left)
                if m_name:
                    hit = True
                    reason = f"Numeric literal {right_desc} in BinOp with measurement variable '{m_name}'"
        elif isinstance(node, ast.AugAssign):
            target_name = find_measurement_name(node.target)
            if target_name:
                val_lit, val_desc = is_numeric_literal_node(node.value)
                if val_lit:
                    hit = True
                    reason = f"Numeric literal {val_desc} in AugAssign on measurement variable '{target_name}'"
                else:
                    for child in ast.walk(node.value):
                        c_lit, c_desc = is_numeric_literal_node(child)
                        if c_lit:
                            hit = True
                            reason = f"Numeric literal {c_desc} in AugAssign on measurement variable '{target_name}'"
                            break

        if hit and hasattr(node, "lineno"):
            lineno = node.lineno
            line_text = source_lines[lineno - 1].strip()
            violations.append(
                (
                    lineno,
                    line_text,
                    f"FORBIDDEN_MEASUREMENT_ARITHMETIC: {reason}",
                )
            )
    return violations


def find_numeric_literals(fpath, source_lines):
    tree = ast.parse("".join(source_lines), filename=fpath)
    hits = []

    # Ignore structural language constructs: Subscript indices, range() arguments, len() comparisons, sys.exit()
    ignored_nodes = set()
    for parent in ast.walk(tree):
        if isinstance(parent, ast.Subscript):
            if isinstance(parent.slice, ast.Constant):
                ignored_nodes.add(parent.slice)
            elif isinstance(parent.slice, ast.UnaryOp) and isinstance(parent.slice.operand, ast.Constant):
                ignored_nodes.add(parent.slice.operand)
        elif isinstance(parent, ast.Call) and isinstance(parent.func, ast.Name) and parent.func.id == "range":
            for arg in parent.args:
                if isinstance(arg, ast.Constant):
                    ignored_nodes.add(arg)
        elif isinstance(parent, ast.Call) and isinstance(parent.func, ast.Name) and parent.func.id in ("exit", "len", "round"):
            for arg in parent.args:
                if isinstance(arg, ast.Constant):
                    ignored_nodes.add(arg)
        elif (
            isinstance(parent, ast.Call)
            and isinstance(parent.func, ast.Attribute)
            and parent.func.attr in ("exit", "group")
        ):
            for arg in parent.args:
                if isinstance(arg, ast.Constant):
                    ignored_nodes.add(arg)

    for node in ast.walk(tree):
        if node in ignored_nodes:
            continue
        hit, val_desc = is_numeric_literal_node(node)
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

        # 1. Strictly forbidden measurement arithmetic: CANNOT be allowlisted
        forbidden = find_forbidden_measurement_arithmetic(fpath, lines)
        seen_forbidden = set()
        for lineno, line_text, reason in forbidden:
            if lineno in seen_forbidden:
                continue
            seen_forbidden.add(lineno)
            violations.append((rel_path, lineno, line_text, reason))

        # 2. General numeric literals: must match allowlist
        hits = find_numeric_literals(fpath, lines)
        seen_lines = set()
        for lineno, line_text, val_desc in hits:
            if lineno in seen_lines or lineno in seen_forbidden:
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
        target_files = [
            os.path.join(ROOT_DIR, "dev/bench/measure_stage_p.py"),
        ]

    violations = check_files(target_files, allowlist)
    if violations:
        print(
            "ERROR: Benchmark literal or measurement integrity violation(s) detected:",
            file=sys.stderr,
        )
        for rel_path, lineno, line_text, val_desc in violations:
            print(
                f"  {rel_path}:{lineno} [{val_desc}]: {line_text}",
                file=sys.stderr,
            )
        print(f"\nTotal violations: {len(violations)}", file=sys.stderr)
        print(
            "Measurement arithmetic with literals is unconditionally forbidden. "
            "All operational literals must have justifications in scripts/bench/literals-allowlist.txt.",
            file=sys.stderr,
        )
        sys.exit(1)

    print(
        f"PASS: All numeric literals in {len(target_files)} benchmark files match allowlist ({len(allowlist)} allowed lines; zero measurement arithmetic)."
    )
    sys.exit(0)


if __name__ == "__main__":
    main()
