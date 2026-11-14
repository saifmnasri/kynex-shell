#!/usr/bin/env bash
# Verify that private files are never tracked in the git repository (D-019/D-020).
set -euo pipefail

TARGET_DIR="${1:-.}"
cd "${TARGET_DIR}"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "ERROR: Not a git work tree" >&2
  exit 1
fi

PRIVATE_PATTERNS=(
  "docs"
  "templates"
  ".agents"
  "SETUP.md"
  "AGENTS.md"
)

mapfile -t TRACKED < <(git ls-files "${PRIVATE_PATTERNS[@]}")

if ((${#TRACKED[@]} > 0)); then
  echo "ERROR: Found ${#TRACKED[@]} private file(s) tracked in git repository (violates D-019/D-020):" >&2
  printf '  - %s\n' "${TRACKED[@]}" >&2
  echo "Untrack them using: git rm -r --cached <path>" >&2
  exit 1
fi

echo "PASS: Zero private files tracked in git repository."
exit 0
