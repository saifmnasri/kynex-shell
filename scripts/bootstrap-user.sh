#!/usr/bin/env bash
# Installs the Rust toolchain and dev/audit tools for the current user (no sudo). Safe to re-run.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

if ! command -v rustup >/dev/null 2>&1; then
  echo "Installing rustup (official installer, downloaded to a temp file first)..."
  tmp="$(mktemp)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o "$tmp"
  sh "$tmp" -y --profile minimal --default-toolchain stable
  rm -f "$tmp"
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"

rustup component add rustfmt clippy llvm-tools-preview
if [[ "${WITH_NIGHTLY:-0}" == "1" ]]; then
  rustup toolchain install nightly --profile minimal   # needed for cargo-fuzz / cargo-udeps
fi

TOOLS=(
  cargo-deny cargo-audit cargo-nextest cargo-llvm-cov cargo-machete cargo-mutants
  cargo-cyclonedx git-cliff typos-cli taplo-cli ripsecrets just
)

if ! command -v cargo-binstall >/dev/null 2>&1; then
  echo "Installing cargo-binstall via release installer..."
  curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash || cargo install cargo-binstall --locked
fi
for t in "${TOOLS[@]}"; do
  bin_name="${t#cargo-}"
  if command -v "$t" >/dev/null 2>&1 || command -v "$bin_name" >/dev/null 2>&1; then
    continue
  fi
  if ! cargo binstall -y --locked "$t"; then
    echo "binstall failed for $t, building from source..."
    cargo install --locked "$t" || echo "WARNING: could not install $t"
  fi
done

if command -v git >/dev/null 2>&1; then
  git config --global --add safe.directory '*' 2>/dev/null || true
fi

if git rev-parse --git-dir >/dev/null 2>&1; then
  git config core.hooksPath .githooks
  chmod +x .githooks/* scripts/*.sh scripts/bench/* dev/*.sh 2>/dev/null || true
  echo "Git hooks enabled (.githooks)."
else
  echo "Not a git repo yet: run 'git init && git config core.hooksPath .githooks'."
fi
echo "Done. Next: ./scripts/check-env.sh"
