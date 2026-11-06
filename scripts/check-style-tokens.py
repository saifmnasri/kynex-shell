#!/usr/bin/env python3
"""Token and style linter for Kynex styles.

Enforces:
1. No org.kde.* imports.
2. No hex color literals (e.g. #1d2029).
3. No named color literals (e.g. color: "red").
4. No functional color constructors (e.g. Qt.rgba, rgb).
5. No numeric literals in styles/**/*.qml except 0 and 1
   (ignoring import lines, comments, and strings).
"""

import os
import re
import sys

HEX_COLOR_RE = re.compile(r"#[0-9a-fA-F]{3,8}\b")
NAMED_COLOR_RE = re.compile(r"(?:color|border\.color)\s*:\s*[\"'][a-zA-Z]+[\"']")
FUNC_COLOR_RE = re.compile(r"\b(?:Qt\.(?:rgba?|hsla?|color)|rgba?|hsla?)\s*\(")
KDE_IMPORT_RE = re.compile(r"^\s*import\s+org\.kde\b")
NUM_RE = re.compile(
    r"(?<![\w$])(?:(?:0[xX][0-9a-fA-F]+)|(?:0[bB][01]+)|(?:0[oO][0-7]+)|"
    r"(?:[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)|(?:\.[0-9]+(?:[eE][+-]?[0-9]+)?))(?![\w$])"
)


def lint_qml_file(filepath: str) -> int:
    try:
        with open(filepath, "r", encoding="utf-8", errors="replace") as fh:
            raw_content = fh.read()
    except OSError as err:
        print(f"ERROR: Cannot read {filepath}: {err}", file=sys.stderr)
        return 1

    violations = 0
    # Strip block comments across multiple lines
    content = re.sub(r"/\*[\s\S]*?\*/", "", raw_content)

    for line_num, line in enumerate(content.splitlines(), 1):
        stripped = line.strip()

        # 1. Reject org.kde.* imports
        if KDE_IMPORT_RE.search(stripped):
            print(
                f"ERROR: {filepath}:{line_num}: forbidden org.kde import: {stripped}",
                file=sys.stderr,
            )
            violations += 1

        # Ignore import lines completely for styling and numeric checks
        if stripped.startswith("import "):
            continue

        # Strip line comments
        line_no_comment = re.sub(r"//.*$", "", line)
        if not line_no_comment.strip():
            continue

        # 2. Reject hex colors
        if HEX_COLOR_RE.search(line_no_comment):
            print(
                f"ERROR: {filepath}:{line_num}: forbidden hex color literal: {stripped}",
                file=sys.stderr,
            )
            violations += 1

        # 3. Reject named colors
        if NAMED_COLOR_RE.search(line_no_comment):
            print(
                f"ERROR: {filepath}:{line_num}: forbidden named color literal: {stripped}",
                file=sys.stderr,
            )
            violations += 1

        # 4. Reject functional color constructors
        if FUNC_COLOR_RE.search(line_no_comment):
            print(
                f"ERROR: {filepath}:{line_num}: forbidden functional color constructor: {stripped}",
                file=sys.stderr,
            )
            violations += 1

        # Strip string literals (both "...", '...', and `...`)
        line_no_str = re.sub(
            r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|`(?:\\.|[^`\\])*`',
            "",
            line_no_comment,
        )

        # 5. Reject every numeric literal except 0 and 1
        for m in NUM_RE.finditer(line_no_str):
            tok = m.group(0)
            if tok not in ("0", "1"):
                print(
                    f"ERROR: {filepath}:{line_num}: forbidden numeric literal {tok!r} (only 0 and 1 allowed): {stripped}",
                    file=sys.stderr,
                )
                violations += 1

    return violations


def main() -> int:
    target_dir = sys.argv[1] if len(sys.argv) > 1 else "styles"
    total_violations = 0

    if os.path.isfile(target_dir) and target_dir.endswith(".qml"):
        total_violations += lint_qml_file(target_dir)
    else:
        for root, _, files in os.walk(target_dir):
            for file in sorted(files):
                if file.endswith(".qml"):
                    filepath = os.path.join(root, file)
                    total_violations += lint_qml_file(filepath)

    if total_violations > 0:
        return min(total_violations, 125)
    return 0


if __name__ == "__main__":
    sys.exit(main())

