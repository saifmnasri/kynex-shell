#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

GUARD_PATTERNS=(
    "scripts/verify.sh"
    "scripts/audit.sh"
    "scripts/check-*.py"
    "scripts/bench/**"
    "scripts/bench/literals-allowlist.txt"
    ".agents/rules/**"
    "deny.toml"
    "_typos.toml"
    ".github/workflows/**"
    "tests/mutation-equivalents.md"
    "docs/work/claims/**"
    "budgets.toml"
)

if [ "${1:-}" = "--list" ]; then
    git diff --name-only origin/main...HEAD -- "${GUARD_PATTERNS[@]}"
    exit 0
fi

echo "=== git diff origin/main...HEAD --stat ==="
git diff origin/main...HEAD --stat

echo ""
echo "=== Guard files diff ==="
git diff origin/main...HEAD -- "${GUARD_PATTERNS[@]}"

echo ""
CHANGED_FILES=$(git diff --name-only origin/main...HEAD -- "${GUARD_PATTERNS[@]}")
if [ -z "${CHANGED_FILES}" ]; then
    COUNT=0
else
    COUNT=$(printf "%s\n" "${CHANGED_FILES}" | grep -c .)
fi

echo "=== Changed guard files count ==="
echo "Changed guard files: ${COUNT}"
