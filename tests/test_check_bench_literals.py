#!/usr/bin/env python3
"""Tests for AST-based numeric literal checker, evasion prevention, and padding rejection.

Verifies that scripts/check-bench-literals.py detects and rejects all metric evasion attempts:
1. scalar literal
2. list-wrapped value
3. tuple value
4. dict value
5. default argument
6. conditional fallback
7. string converted to float
8. string converted to int
9. old Stage P fallback [12.45] rejected against allowlist
10. legitimate operational delay accepted in allowlist
11-14. padding patterns (+ 1.2, + 0.8, += 1.2, allowlist bypass attempt) strictly rejected
"""

import importlib.util
import os
import sys

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
CHECKER_PATH = os.path.join(ROOT_DIR, "scripts/check-bench-literals.py")

# Import check-bench-literals dynamically
spec = importlib.util.spec_from_file_location("check_bench_literals", CHECKER_PATH)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def check_snippet(code_snippet, allowlist=None):
    if allowlist is None:
        allowlist = set()
    lines = code_snippet.splitlines(keepends=True)
    hits = checker.find_numeric_literals("dummy.py", lines)
    forbidden = checker.find_forbidden_measurement_arithmetic("dummy.py", lines)
    violations = []
    seen = set()
    for lineno, line_text, reason in forbidden:
        if lineno in seen:
            continue
        seen.add(lineno)
        violations.append((lineno, line_text, reason))
    for lineno, line_text, val_desc in hits:
        if lineno in seen:
            continue
        seen.add(lineno)
        if ("dummy.py", line_text) not in allowlist:
            violations.append((lineno, line_text, val_desc))
    return violations


def test_evasion_scalar():
    code = "latency_p95 = 12.45\n"
    v = check_snippet(code)
    assert len(v) == 1, f"Expected scalar evasion to be detected, got: {v}"
    assert "12.45" in v[0][2]
    print("PASS: test_evasion_scalar")


def test_evasion_list_wrapped():
    code = "m_500_samples = [12.45]\n"
    v = check_snippet(code)
    assert len(v) == 1, f"Expected list-wrapped evasion to be detected, got: {v}"
    assert "12.45" in v[0][2]
    print("PASS: test_evasion_list_wrapped")


def test_evasion_tuple():
    code = "results = (12.45,)\n"
    v = check_snippet(code)
    assert len(v) == 1, f"Expected tuple evasion to be detected, got: {v}"
    assert "12.45" in v[0][2]
    print("PASS: test_evasion_tuple")


def test_evasion_dict_value():
    code = "metrics = {'latency': 12.45}\n"
    v = check_snippet(code)
    assert len(v) == 1, f"Expected dict value evasion to be detected, got: {v}"
    assert "12.45" in v[0][2]
    print("PASS: test_evasion_dict_value")


def test_evasion_default_argument():
    code = "def measure(fallback=12.45):\n    return fallback\n"
    v = check_snippet(code)
    assert len(v) == 1, f"Expected default argument evasion to be detected, got: {v}"
    assert "12.45" in v[0][2]
    print("PASS: test_evasion_default_argument")


def test_evasion_conditional_fallback():
    code = "val = sample if sample else 12.45\n"
    v = check_snippet(code)
    assert len(v) == 1, f"Expected conditional fallback evasion to be detected, got: {v}"
    assert "12.45" in v[0][2]
    print("PASS: test_evasion_conditional_fallback")


def test_evasion_string_converted_to_float():
    code = "val = float('12.45')\n"
    v = check_snippet(code)
    assert len(v) == 1, f"Expected float('12.45') evasion to be detected, got: {v}"
    assert "float('12.45')" in v[0][2]
    print("PASS: test_evasion_string_converted_to_float")


def test_evasion_string_converted_to_int():
    code = "val = int('42')\n"
    v = check_snippet(code)
    assert len(v) == 1, f"Expected int('42') evasion to be detected, got: {v}"
    assert "int('42')" in v[0][2]
    print("PASS: test_evasion_string_converted_to_int")


def test_old_code_literal_rejected_against_real_allowlist():
    allowlist = checker.load_allowlist(checker.ALLOWLIST_PATH)
    rel_path = "dev/bench/measure_stage_p.py"
    bad_line = "m_500_samples = [12.45]"
    assert (rel_path, bad_line) not in allowlist, "Old literal [12.45] must NOT be allowlisted"
    print("PASS: test_old_code_literal_rejected_against_real_allowlist")


def test_allowlisted_operational_literals_accepted():
    allowlist = checker.load_allowlist(checker.ALLOWLIST_PATH)
    rel_path = "dev/bench/measure_stage_p.py"
    good_line = "time.sleep(0.05)"
    assert (rel_path, good_line) in allowlist, f"{good_line} must be allowlisted"
    print("PASS: test_allowlisted_operational_literals_accepted")


def test_padding_pattern_binop_shell_lat():
    code = "e2e_ms = shell_lat_ms + 1.2\n"
    v = check_snippet(code)
    assert len(v) >= 1, f"Expected padding pattern (+ 1.2) to be rejected, got: {v}"
    assert any("FORBIDDEN_MEASUREMENT_ARITHMETIC" in desc or "1.2" in desc for _, _, desc in v)
    print("PASS: test_padding_pattern_binop_shell_lat")


def test_padding_pattern_binop_reverse():
    code = "e2e_ms = 0.8 + shell_lat_ms\n"
    v = check_snippet(code)
    assert len(v) >= 1, f"Expected padding pattern (0.8 +) to be rejected, got: {v}"
    assert any("FORBIDDEN_MEASUREMENT_ARITHMETIC" in desc or "0.8" in desc for _, _, desc in v)
    print("PASS: test_padding_pattern_binop_reverse")


def test_padding_pattern_augassign():
    code = "latency_ms += 1.2\n"
    v = check_snippet(code)
    assert len(v) >= 1, f"Expected padding pattern (+= 1.2) to be rejected, got: {v}"
    assert any("FORBIDDEN_MEASUREMENT_ARITHMETIC" in desc or "1.2" in desc for _, _, desc in v)
    print("PASS: test_padding_pattern_augassign")


def test_padding_pattern_cannot_be_allowlisted():
    allowlist = {("dummy.py", "e2e_ms = shell_lat_ms + 1.2")}
    code = "e2e_ms = shell_lat_ms + 1.2\n"
    v = check_snippet(code, allowlist=allowlist)
    assert len(v) >= 1, f"Padding pattern must FAIL even if attempted in allowlist, got: {v}"
    assert any("FORBIDDEN_MEASUREMENT_ARITHMETIC" in desc for _, _, desc in v)
    print("PASS: test_padding_pattern_cannot_be_allowlisted")


def main():
    test_evasion_scalar()
    test_evasion_list_wrapped()
    test_evasion_tuple()
    test_evasion_dict_value()
    test_evasion_default_argument()
    test_evasion_conditional_fallback()
    test_evasion_string_converted_to_float()
    test_evasion_string_converted_to_int()
    test_old_code_literal_rejected_against_real_allowlist()
    test_allowlisted_operational_literals_accepted()
    test_padding_pattern_binop_shell_lat()
    test_padding_pattern_binop_reverse()
    test_padding_pattern_augassign()
    test_padding_pattern_cannot_be_allowlisted()
    print("\nALL 14 check-bench-literals evasion and padding tests PASSED!")


if __name__ == "__main__":
    main()
