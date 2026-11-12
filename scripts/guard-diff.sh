#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

# Ensure safe.directory is configured in containerized CI environments (where container user != checkout user)
git config --global --add safe.directory "${ROOT_DIR}" 2>/dev/null || true
git config --global --add safe.directory "*" 2>/dev/null || true

BASE="origin/main"
if ! git rev-parse --verify "${BASE}" >/dev/null 2>&1; then
    if git rev-parse --verify main >/dev/null 2>&1; then
        BASE="main"
    elif git rev-parse --verify remotes/origin/main >/dev/null 2>&1; then
        BASE="remotes/origin/main"
    fi
fi

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
    git diff --name-only "${BASE}...HEAD" -- "${GUARD_PATTERNS[@]}"
    exit 0
fi

echo "=== git diff origin/main...HEAD --stat ==="
git diff "${BASE}...HEAD" --stat

echo ""
echo "=== Guard files diff ==="
git diff "${BASE}...HEAD" -- "${GUARD_PATTERNS[@]}"

echo ""
CHANGED_FILES=$(git diff --name-only "${BASE}...HEAD" -- "${GUARD_PATTERNS[@]}")
if [ -z "${CHANGED_FILES}" ]; then
    COUNT=0
else
    COUNT=$(printf "%s\n" "${CHANGED_FILES}" | grep -c .)
fi

echo "=== Changed guard files count ==="
echo "Changed guard files: ${COUNT}"
