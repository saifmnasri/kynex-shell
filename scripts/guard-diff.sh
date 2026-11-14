#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

export GIT_CONFIG_COUNT=1
export GIT_CONFIG_KEY_0=safe.directory
export GIT_CONFIG_VALUE_0="*"

command -v git >/dev/null 2>&1 && git config --system --add safe.directory "*" 2>/dev/null || true
command -v git >/dev/null 2>&1 && git config --global --add safe.directory "*" 2>/dev/null || true

if [ ! -e "${ROOT_DIR}/.git" ] || ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "ERROR: Not a git repository. Cannot determine guard diff without git history." >&2
    exit 1
fi

LIST_ONLY=0
EXPLICIT_RANGE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --list)
            LIST_ONLY=1
            shift
            ;;
        *)
            EXPLICIT_RANGE="$1"
            shift
            ;;
    esac
done

BASE=""
HEAD_REF="HEAD"

if [ -n "${EXPLICIT_RANGE}" ]; then
    if [[ "${EXPLICIT_RANGE}" == *"..."* ]]; then
        BASE="${EXPLICIT_RANGE%...*}"
        HEAD_REF="${EXPLICIT_RANGE#*...}"
    elif [[ "${EXPLICIT_RANGE}" == *".."* ]]; then
        BASE="${EXPLICIT_RANGE%..*}"
        HEAD_REF="${EXPLICIT_RANGE#*..}"
    else
        BASE="${EXPLICIT_RANGE}"
        HEAD_REF="HEAD"
    fi
elif [ -n "${GUARD_DIFF_RANGE:-}" ]; then
    if [[ "${GUARD_DIFF_RANGE}" == *"..."* ]]; then
        BASE="${GUARD_DIFF_RANGE%...*}"
        HEAD_REF="${GUARD_DIFF_RANGE#*...}"
    elif [[ "${GUARD_DIFF_RANGE}" == *".."* ]]; then
        BASE="${GUARD_DIFF_RANGE%..*}"
        HEAD_REF="${GUARD_DIFF_RANGE#*..}"
    else
        BASE="${GUARD_DIFF_RANGE}"
        HEAD_REF="HEAD"
    fi
elif [ -n "${GITHUB_BASE_REF:-}" ]; then
    BASE=$(git merge-base HEAD "origin/${GITHUB_BASE_REF}" 2>/dev/null || git merge-base HEAD "${GITHUB_BASE_REF}" 2>/dev/null || true)
    if [ -z "${BASE}" ]; then
        echo "ERROR: Failed to determine merge-base with GITHUB_BASE_REF '${GITHUB_BASE_REF}'. Full git history is required." >&2
        exit 1
    fi
    HEAD_REF="HEAD"
elif [ "${GITHUB_EVENT_NAME:-}" = "push" ] && [ -n "${GITHUB_EVENT_BEFORE:-}" ] && ! [[ "${GITHUB_EVENT_BEFORE}" =~ ^0+$ ]]; then
    BASE="${GITHUB_EVENT_BEFORE}"
    HEAD_REF="HEAD"
else
    CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || true)
    if [ "${CURRENT_BRANCH}" != "main" ] && [ "${CURRENT_BRANCH}" != "HEAD" ] && [ -n "${CURRENT_BRANCH}" ]; then
        BASE=$(git merge-base HEAD origin/main 2>/dev/null || git merge-base HEAD main 2>/dev/null || true)
        if [ -z "${BASE}" ]; then
            echo "ERROR: Failed to determine merge-base with main. Ensure main or origin/main exists in history." >&2
            exit 1
        fi
        HEAD_REF="HEAD"
    else
        BASE=""
        HEAD_REF="HEAD"
    fi
fi

if [ -n "${BASE}" ] && ! git rev-parse --verify "${BASE}" >/dev/null 2>&1; then
    echo "ERROR: Base revision '${BASE}' cannot be resolved. Git history may be shallow." >&2
    exit 1
fi

if ! git rev-parse --verify "${HEAD_REF}" >/dev/null 2>&1; then
    echo "ERROR: Head revision '${HEAD_REF}' cannot be resolved." >&2
    exit 1
fi

GUARD_PATTERNS=(
    "scripts/verify.sh"
    "scripts/audit.sh"
    "scripts/check-*.py"
    "scripts/check-*.sh"
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

if [ -z "${BASE}" ]; then
    if [ "${LIST_ONLY}" -eq 1 ]; then
        exit 0
    fi
    echo "Notice: On main branch with no diff range specified."
    echo "Changed guard files: 0"
    exit 0
fi

if [ "${LIST_ONLY}" -eq 1 ]; then
    git diff --name-only "${BASE}...${HEAD_REF}" -- "${GUARD_PATTERNS[@]}"
    exit 0
fi

echo "=== git diff ${BASE}...${HEAD_REF} --stat ==="
git diff "${BASE}...${HEAD_REF}" --stat

echo ""
echo "=== Guard files diff ==="
git diff "${BASE}...${HEAD_REF}" -- "${GUARD_PATTERNS[@]}"

echo ""
CHANGED_FILES=$(git diff --name-only "${BASE}...${HEAD_REF}" -- "${GUARD_PATTERNS[@]}")
if [ -z "${CHANGED_FILES}" ]; then
    COUNT=0
else
    COUNT=$(printf "%s\n" "${CHANGED_FILES}" | grep -c .)
fi

echo "=== Changed guard files count ==="
echo "Changed guard files: ${COUNT}"
