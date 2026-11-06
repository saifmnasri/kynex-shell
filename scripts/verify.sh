#!/usr/bin/env bash
# The quality gate. Usage: scripts/verify.sh [--fast] ; STRICT=1 turns SKIPs into failures.
cd "$(dirname "$0")/.." || exit 1
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
FAST=0; [[ "${1:-}" == "--fast" ]] && FAST=1
pass=(); failed=(); skipped=()

step() { # step "name" cmd...
  local name="$1"; shift
  echo; echo "=== $name ==="
  if "$@"; then pass+=("$name"); else failed+=("$name"); fi
}
skip() { echo; echo "=== $1 === SKIPPED ($2)"; skipped+=("$1: $2"); }
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
  local target_dir="${1:-styles}"
  ./scripts/check-style-tokens.py "$target_dir"
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
    QMLLINT_ARGS=(-I crates/kynex-shell/qml)
    if "$QMLLINT" --help 2>&1 | grep -q -- '--compiler'; then
      QMLLINT_ARGS+=(--compiler warning)
    fi
    step "qmllint" "$QMLLINT" "${QMLLINT_ARGS[@]}" "${QMLFILES[@]}"
  else
    skip "qmllint" "not installed"
  fi
fi

mapfile -t SHFILES < <(find scripts dev .githooks -type f \( -name '*.sh' -o -path '.githooks/*' \) 2>/dev/null)
have shellcheck && ((${#SHFILES[@]})) && step "shellcheck" shellcheck "${SHFILES[@]}" || true

mapfile -t DESKTOP < <(find packaging -name '*.desktop' 2>/dev/null)
if ((${#DESKTOP[@]})); then
  have desktop-file-validate && step "desktop-file-validate" desktop-file-validate "${DESKTOP[@]}" || skip "desktop-file-validate" "not installed"
fi

echo; echo "================ SUMMARY ================"
printf 'PASSED : %s\n' "${#pass[@]}"; ((${#pass[@]})) && printf '  + %s\n' "${pass[@]}"
printf 'SKIPPED: %s\n' "${#skipped[@]}"; ((${#skipped[@]})) && printf '  ~ %s\n' "${skipped[@]}"
printf 'FAILED : %s\n' "${#failed[@]}"; ((${#failed[@]})) && printf '  - %s\n' "${failed[@]}"
((${#failed[@]})) && exit 1
[[ "${STRICT:-0}" == "1" && ${#skipped[@]} -gt 0 ]] && { echo "STRICT: skips not allowed"; exit 1; }
exit 0
