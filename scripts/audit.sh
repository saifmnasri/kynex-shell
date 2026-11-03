#!/usr/bin/env bash
# Deep audit: full verify + dependency/security/code-smell checks. Run at the end of each phase.
cd "$(dirname "$0")/.." || exit 1
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
have() { command -v "$1" >/dev/null 2>&1; }
rc=0
echo "### 1. Full verify (STRICT)"; STRICT=1 ./scripts/verify.sh || rc=1

if [[ -f Cargo.toml ]]; then
  echo; echo "### 2. Code smells in non-test Rust code"
  grep -RIn --include='*.rs' -E '\.unwrap\(\)|\bunsafe\b|todo!|unimplemented!|dbg!|println!|TODO|FIXME|HACK' crates 2>/dev/null \
    | grep -v -E '/tests?/|_test\.rs|#\[cfg\(test\)\]' || echo "none found"

  echo; echo "### 3. Unused dependencies";   have cargo-machete && cargo machete || echo "cargo-machete not installed"
  echo; echo "### 4. Coverage (target >= 80% lines)"
  have cargo-llvm-cov && cargo llvm-cov --workspace --summary-only || echo "cargo-llvm-cov not installed"
  echo; echo "### 5. SBOM"
  mkdir -p dev/tmp && have cargo-cyclonedx && cargo cyclonedx --format json --override-filename sbom 2>/dev/null && echo "SBOM written" || echo "cargo-cyclonedx not installed"
  echo; echo "### 6. Duplicate crate versions"; cargo tree --workspace -d 2>/dev/null | head -40
fi
echo; echo "### 7. Manual review (agent): re-read the diff as a hostile reviewer; fill docs/SECURITY_CHECKLIST.md for new code"
exit $rc
