#!/usr/bin/env bash
# One-time git + GitHub identity setup: login, SSH key, commit signing, name/email. Safe to re-run.
# Interactive part: GitHub login (device code in your browser).
set -euo pipefail

command -v gh >/dev/null 2>&1 || { echo "gh not installed: run 'sudo ./scripts/bootstrap-system.sh' first" >&2; exit 1; }

if ! gh auth status >/dev/null 2>&1; then
  echo "Logging in to GitHub (follow the browser/device-code instructions)..."
  gh auth login --hostname github.com --git-protocol ssh --web
fi
gh auth refresh --hostname github.com --scopes admin:public_key,admin:ssh_signing_key,repo,workflow || true

key="$HOME/.ssh/id_ed25519"
mkdir -p "$HOME/.ssh" && chmod 700 "$HOME/.ssh"
if [[ ! -f "$key" ]]; then
  # Passphrase-less key: acceptable for a throwaway dev VM only. Use a passphrase on real machines.
  ssh-keygen -t ed25519 -N "" -C "kynex-dev@$(hostname)" -f "$key"
fi

title="kynex-dev-$(hostname)"
gh ssh-key add "$key.pub" --title "$title" --type authentication 2>/dev/null || echo "(authentication key already on GitHub)"
gh ssh-key add "$key.pub" --title "$title-signing" --type signing 2>/dev/null || echo "(signing key already on GitHub)"

login="$(gh api user --jq .login)"
uid="$(gh api user --jq .id)"
name="$(gh api user --jq '.name // .login')"
email="${uid}+${login}@users.noreply.github.com"

[[ -n "$(git config --global user.name || true)" ]] || git config --global user.name "$name"
[[ -n "$(git config --global user.email || true)" ]] || git config --global user.email "$email"
git config --global init.defaultBranch main
git config --global gpg.format ssh
git config --global user.signingkey "$key.pub"
git config --global commit.gpgsign true
git config --global tag.gpgsign true
git config --global pull.ff only
git config --global push.autoSetupRemote true

printf '%s %s\n' "$(git config --global user.email)" "$(cat "$key.pub")" > "$HOME/.ssh/allowed_signers"
git config --global gpg.ssh.allowedSignersFile "$HOME/.ssh/allowed_signers"

echo "Git ready: $(git config --global user.name) <$(git config --global user.email)>, SSH signing on."
