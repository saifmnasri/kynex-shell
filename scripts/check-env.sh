#!/usr/bin/env bash
# Prints PASS/FAIL for everything the project needs. Exit code 1 if any FAIL.
cd "$(dirname "$0")/.." || exit 1
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
fail=0
ok()   { printf 'PASS  %s\n' "$1"; }
bad()  { printf 'FAIL  %s\n' "$1"; fail=1; }
warn() { printf 'WARN  %s\n' "$1"; }

need_cmd() { if command -v "$1" >/dev/null 2>&1; then ok "$1 ($("$1" --version 2>&1 | head -1))"; else bad "$1 missing"; fi; }
opt_cmd()  { if command -v "$1" >/dev/null 2>&1; then ok "$1"; else warn "$1 missing (optional)"; fi; }

echo "== OS =="; . /etc/os-release; echo "$PRETTY_NAME ($(uname -m))"
[[ "$VERSION_ID" == "26.04" ]] && ok "Ubuntu 26.04" || warn "expected Ubuntu 26.04, found $VERSION_ID"

echo "== Core tools =="
for c in git gh cmake ninja pkg-config clang rustc cargo rustup jq; do need_cmd "$c"; done
echo "== Rust tools =="
for c in cargo-deny cargo-audit cargo-nextest cargo-llvm-cov git-cliff typos ripsecrets; do need_cmd "$c"; done
for c in cargo-machete cargo-mutants cargo-cyclonedx taplo just; do opt_cmd "$c"; done
echo "== Qt / KWin =="
for m in Qt6Core Qt6Gui Qt6Qml Qt6Quick Qt6WaylandClient; do
  pkg-config --exists "$m" 2>/dev/null && ok "$m $(pkg-config --modversion "$m")" || bad "$m dev files missing"
done
need_cmd kwin_wayland
dpkg -l 2>/dev/null | grep -qi layershellqt && ok "LayerShellQt installed" || bad "LayerShellQt dev package missing"
command -v qmllint >/dev/null 2>&1 && ok qmllint || { [[ -x /usr/lib/qt6/bin/qmllint ]] && ok "qmllint (/usr/lib/qt6/bin)" || warn "qmllint not on PATH"; }
echo "== Services =="
for s in upower NetworkManager bluetooth; do systemctl list-unit-files 2>/dev/null | grep -q "^$s" && ok "$s unit present" || warn "$s unit not found"; done
echo "== Git =="
gh auth status >/dev/null 2>&1 && ok "GitHub login" || warn "not logged in to GitHub (run scripts/setup-git.sh)"
git config user.name >/dev/null && ok "git user.name" || bad "git user.name not set"
git config user.email >/dev/null && ok "git user.email" || bad "git user.email not set"
[[ "$(git config commit.gpgsign)" == "true" ]] && ok "commit signing on" || warn "commit signing off"
[[ "$(git config core.hooksPath)" == ".githooks" ]] && ok "hooks enabled" || warn "hooks not enabled"
echo
((fail)) && { echo "Environment NOT ready."; exit 1; } || echo "Environment ready."
