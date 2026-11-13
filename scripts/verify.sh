#!/usr/bin/env bash
# The quality gate. Usage: scripts/verify.sh [--fast] [--ci] ; STRICT=1 turns unallowed SKIPs into failures.
cd "$(dirname "$0")/.." || exit 1
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
export GIT_CONFIG_COUNT=1
export GIT_CONFIG_KEY_0=safe.directory
export GIT_CONFIG_VALUE_0="*"
command -v git >/dev/null 2>&1 && git config --system --add safe.directory "*" 2>/dev/null || true
command -v git >/dev/null 2>&1 && git config --global --add safe.directory "*" 2>/dev/null || true
FAST=0
CI_MODE=0
for arg in "$@"; do
  case "$arg" in
    --fast) FAST=1 ;;
    --ci)   CI_MODE=1 ;;
  esac
done
pass=(); failed=(); skipped=(); skipped_local=()

step() { # step "name" cmd...
  local name="$1"; shift
  echo; echo "=== $name ==="
  if "$@"; then pass+=("$name"); else failed+=("$name"); fi
}
skip() { echo; echo "=== $1 === SKIPPED ($2)"; skipped+=("$1: $2"); }
skip_local() { echo; echo "=== $1 === SKIPPED (local-only: $2)"; skipped+=("$1 (local-only: $2)"); skipped_local+=("$1"); }
have() { command -v "$1" >/dev/null 2>&1; }

HAS_RUST=0; [[ -f Cargo.toml ]] && HAS_RUST=1

if ((HAS_RUST)); then
  step "rustfmt" cargo fmt --all -- --check
  step "clippy" cargo clippy --workspace --all-targets --locked -- -D warnings
  if have cargo-nextest; then step "tests (nextest)" cargo nextest run --workspace --locked
  else step "tests" cargo test --workspace --locked; fi
  step "doc tests" cargo test --workspace --doc --locked
  if ((FAST)); then
    skip "cargo-deny / cargo-audit" "--fast"
  else
    have cargo-deny  && step "cargo-deny (licenses, bans, advisories)" cargo deny check || skip "cargo-deny" "not installed"
    have cargo-audit && step "cargo-audit (RustSec)" cargo audit || skip "cargo-audit" "not installed"
  fi
else
  skip "rust steps" "no Cargo.toml yet"
fi

have typos && step "typos" typos || skip "typos" "not installed"
have ripsecrets && step "secret scan" ripsecrets --strict-ignore . || skip "secret scan" "ripsecrets not installed"

# shellcheck disable=SC2317,SC2329
check_style_tokens() {
  ./scripts/check-style-tokens.py "styles" && ./scripts/check-style-tokens.py "crates/kynex-shell/qml"
}

step "style tokens lint" check_style_tokens
[[ -f scripts/test-token-lint.sh ]] && step "style tokens linter test" ./scripts/test-token-lint.sh

export QT_SELECT=qt6
QMLLINT=""
if [[ -x /usr/lib/qt6/bin/qmllint ]]; then
  QMLLINT="/usr/lib/qt6/bin/qmllint"
elif have qmllint; then
  QMLLINT="qmllint"
fi

mapfile -t QMLFILES < <(find crates styles -name '*.qml' 2>/dev/null)
if ((${#QMLFILES[@]})); then
  if [[ -n "$QMLLINT" ]]; then
    QMLLINT_ARGS=(-I crates/kynex-shell/qml --compiler warning --max-warnings 0)
    step "qmllint" "$QMLLINT" "${QMLLINT_ARGS[@]}" "${QMLFILES[@]}"
  else
    skip "qmllint" "not installed"
  fi
fi

# shellcheck disable=SC2317,SC2329
check_qml_wait_calls() {
  local exc_file="tests/qml_tests/wait_exceptions.txt"
  local -a allowed=()
  if [[ -f "$exc_file" ]]; then
    mapfile -t allowed < <(grep -v '^\s*#' "$exc_file" | grep -v '^\s*$' || true)
    if ((${#allowed[@]} > 3)); then
      echo "ERROR: tests/qml_tests/wait_exceptions.txt has ${#allowed[@]} exceptions; maximum allowed is 3" >&2
      return 1
    fi
  fi

  local wait_hits
  wait_hits=$(grep -rnE '\bwait\(' tests/qml_tests/*.qml 2>/dev/null || true)
  if [[ -z "$wait_hits" ]]; then
    return 0
  fi

  local unallowed=()
  while IFS= read -r hit; do
    [[ -z "$hit" ]] && continue
    local is_exc=0
    for exc in "${allowed[@]}"; do
      if [[ "$hit" == *"$exc"* ]]; then
        is_exc=1
        break
      fi
    done
    if ((!is_exc)); then
      unallowed+=("$hit")
    fi
  done <<< "$wait_hits"

  if ((${#unallowed[@]} > 0)); then
    echo "ERROR: Found unexempted wait() calls in tests/qml_tests/*.qml:" >&2
    printf '  %s\n' "${unallowed[@]}" >&2
    echo "Use tryVerify/tryCompare or register a justified exception (max 3) in tests/qml_tests/wait_exceptions.txt" >&2
    return 1
  fi
  return 0
}

step "qml wait check" check_qml_wait_calls

mapfile -t SHFILES < <(find scripts dev .githooks -type f \( -name '*.sh' -o -path '.githooks/*' \) ! -path 'dev/tmp/*' 2>/dev/null)
have shellcheck && ((${#SHFILES[@]})) && step "shellcheck" shellcheck "${SHFILES[@]}" || true

mapfile -t DESKTOP < <(find packaging -name '*.desktop' 2>/dev/null)
if ((${#DESKTOP[@]})); then
  have desktop-file-validate && step "desktop-file-validate" desktop-file-validate "${DESKTOP[@]}" || skip "desktop-file-validate" "not installed"
fi

if [[ -f scripts/test-qml-controls.sh ]]; then
  step "qml quick test (base controls)" ./scripts/test-qml-controls.sh
fi

if [[ -f scripts/visual-test.sh ]]; then
  step "visual regression test (gallery)" ./scripts/visual-test.sh
fi

if [[ -f scripts/mutation-test-qml.sh ]]; then
  step "adversarial mutation test (qml controls)" ./scripts/mutation-test-qml.sh
fi

if [[ -f scripts/test-systemd-unit.sh ]] && systemctl --user list-units >/dev/null 2>&1; then
  step "systemd user unit sandbox test" ./scripts/test-systemd-unit.sh
fi

if ((CI_MODE)); then
  skip_local "work report evidence check" "reports are private, D-019/D-020"
elif [[ -f scripts/check-report.py ]]; then
  step "work report evidence check" ./scripts/check-report.py
fi

if [[ -f scripts/check-bench-literals.py ]]; then
  step "ast benchmark literal check" ./scripts/check-bench-literals.py
fi

if [[ -f tests/test_check_bench_literals.py ]]; then
  step "ast benchmark literal evasion test" python3 tests/test_check_bench_literals.py
fi

if [[ -f tests/test_budget_scalar_rejection.py ]]; then
  step "budget-check scalar rejection test" python3 tests/test_budget_scalar_rejection.py
fi

if [[ -f scripts/check-private-paths.sh ]]; then
  step "private paths untracked check" ./scripts/check-private-paths.sh
fi

if [[ -f tests/test_check_private_paths.py ]]; then
  step "private paths guard test" python3 tests/test_check_private_paths.py
fi

echo; echo "================ SUMMARY ================"
printf 'PASSED : %s\n' "${#pass[@]}"; ((${#pass[@]})) && printf '  + %s\n' "${pass[@]}"
printf 'SKIPPED: %s\n' "${#skipped[@]}"; ((${#skipped[@]})) && printf '  ~ %s\n' "${skipped[@]}"
printf 'FAILED : %s\n' "${#failed[@]}"; ((${#failed[@]})) && printf '  - %s\n' "${failed[@]}"
((${#failed[@]})) && exit 1

if [[ "${STRICT:-0}" == "1" ]]; then
  unallowed_skips=0
  for s in "${skipped[@]}"; do
    if [[ "$s" != *"(local-only:"* ]]; then
      ((unallowed_skips++)) || true
    fi
  done
  if ((unallowed_skips > 0)); then
    echo "STRICT: unallowed skips not permitted ($unallowed_skips unallowed)"
    exit 1
  fi
fi
exit 0
