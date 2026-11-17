#!/usr/bin/env bash
# uninstall-session.sh: Uninstalls Kynex Shell session files recorded in manifest.
# Supports --dry-run and DESTDIR for staging/testing.
# Part of Kynex Shell. Public-safe, no hardcoded private paths.
set -euo pipefail

DESTDIR="${DESTDIR:-}"
DRY_RUN=0

for arg in "$@"; do
  case "$arg" in
    --dry-run)
      DRY_RUN=1
      ;;
    --help|-h)
      echo "Usage: $0 [--dry-run]"
      echo "Environment variables:"
      echo "  DESTDIR=/staging/root  Uninstall from target root prefix"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

MANIFEST_FILE="${DESTDIR}/etc/kynex/session-manifest.txt"

execute() {
  echo "+ $*"
  if [[ "$DRY_RUN" -eq 0 ]]; then
    "$@"
  fi
}

echo "=== Kynex Session Uninstallation ==="
echo "DESTDIR:  ${DESTDIR:-<root>}"
echo "Manifest: $MANIFEST_FILE"
echo "Dry run:  $DRY_RUN"

if [[ ! -f "$MANIFEST_FILE" ]]; then
  echo "Notice: Manifest file not found: $MANIFEST_FILE. Nothing to uninstall."
  exit 0
fi

# Track parent directories for clean cleanup
declare -A DIRS_TO_CHECK=()

# 1. Remove recorded files
while IFS= read -r file || [[ -n "$file" ]]; do
  [[ -z "$file" ]] && continue
  target="${DESTDIR}${file}"
  if [[ -f "$target" || -L "$target" ]]; then
    execute rm -f "$target"
    DIRS_TO_CHECK["$(dirname "$target")"]=1
  fi
done < "$MANIFEST_FILE"

# 2. Clean empty parent directories
for dir in "${!DIRS_TO_CHECK[@]}"; do
  cur="$dir"
  while [[ -n "$cur" && "$cur" != "${DESTDIR}/" && "$cur" != "/" && "$cur" != "." ]]; do
    if [[ -d "$cur" ]] && [[ -z "$(ls -A "$cur" 2>/dev/null || true)" ]]; then
      execute rmdir "$cur" 2>/dev/null || true
      cur="$(dirname "$cur")"
    else
      break
    fi
  done
done

# 3. Remove manifest itself
execute rm -f "$MANIFEST_FILE"
cur="$(dirname "$MANIFEST_FILE")"
while [[ -n "$cur" && "$cur" != "${DESTDIR:-/}" && "$cur" != "${DESTDIR}" && "$cur" != "/" && "$cur" != "." ]]; do
  if [[ -d "$cur" ]] && [[ -z "$(ls -A "$cur" 2>/dev/null || true)" ]]; then
    execute rmdir "$cur" 2>/dev/null || true
    cur="$(dirname "$cur")"
  else
    break
  fi
done

echo "Uninstallation complete."
