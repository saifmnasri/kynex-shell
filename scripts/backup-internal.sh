#!/usr/bin/env bash
# Synchronizes internal docs, templates, prompts, and agent rules to a private repository.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_DIR="${HOME}/kynexos-internal"

echo "=== Backing up internal files to $TARGET_DIR ==="

mkdir -p "$TARGET_DIR"

# Initialize git repository if not already present
if [[ ! -d "$TARGET_DIR/.git" ]]; then
  echo "Initializing git repository in $TARGET_DIR..."
  git -C "$TARGET_DIR" init -b main
fi

# Copy hidden / internal assets
echo "Copying internal directories and files..."
mkdir -p "$TARGET_DIR/docs" "$TARGET_DIR/templates" "$TARGET_DIR/.agents"

# Copy directories
if [[ -d "$ROOT_DIR/docs" ]]; then
  cp -a "$ROOT_DIR/docs/." "$TARGET_DIR/docs/"
fi
if [[ -d "$ROOT_DIR/templates" ]]; then
  cp -a "$ROOT_DIR/templates/." "$TARGET_DIR/templates/"
fi
if [[ -d "$ROOT_DIR/.agents" ]]; then
  cp -a "$ROOT_DIR/.agents/." "$TARGET_DIR/.agents/"
fi

# Copy standalone internal files
for file in SETUP.md AGENTS.md; do
  if [[ -f "$ROOT_DIR/$file" ]]; then
    cp -a "$ROOT_DIR/$file" "$TARGET_DIR/"
  fi
done

# Ensure private GitHub remote exists
if ! git -C "$TARGET_DIR" remote get-url origin >/dev/null 2>&1; then
  echo "Checking remote GitHub repo..."
  if ! gh repo view kynexos-internal >/dev/null 2>&1; then
    echo "Creating private remote repository kynexos-internal..."
    gh repo create kynexos-internal --private
  fi
  REMOTE_URL="$(gh repo view kynexos-internal --json sshUrl -q .sshUrl 2>/dev/null || echo "git@github.com:$(gh api user -q .login)/kynexos-internal.git")"
  git -C "$TARGET_DIR" remote add origin "$REMOTE_URL"
fi

# Verify remote repository is private
VISIBILITY="$(gh repo view kynexos-internal --json visibility -q .visibility)"
if [[ "$VISIBILITY" != "PRIVATE" ]]; then
  echo "ERROR: kynexos-internal remote visibility is '$VISIBILITY', expected 'PRIVATE'!" >&2
  exit 1
fi
echo "Verified remote repo visibility: $VISIBILITY"

# Commit changes in target repository
git -C "$TARGET_DIR" add -A
if ! git -C "$TARGET_DIR" diff --cached --quiet; then
  echo "Committing updates to internal repository..."
  git -C "$TARGET_DIR" commit -m "chore(internal): synchronize internal specifications and agent rules"
else
  echo "No changes to commit in internal repository."
fi

# Push to origin
CURRENT_BRANCH="$(git -C "$TARGET_DIR" branch --show-current || echo "main")"
echo "Pushing to origin $CURRENT_BRANCH..."
git -C "$TARGET_DIR" push -u origin "$CURRENT_BRANCH"

echo "=== Backup completed successfully ==="
