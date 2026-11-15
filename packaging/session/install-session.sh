#!/usr/bin/env bash
# install-session.sh: Installs Kynex Shell session files into system paths.
# Supports --dry-run and DESTDIR for staging/testing.
# Part of Kynex Shell. Public-safe, no hardcoded private paths.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

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
      echo "  DESTDIR=/staging/root  Install into target root prefix"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

MANIFEST_DIR="${DESTDIR}/etc/kynex"
MANIFEST_FILE="${MANIFEST_DIR}/session-manifest.txt"

# Temporary file to build fresh manifest
TMP_MANIFEST="$(mktemp)"
trap 'rm -f "$TMP_MANIFEST"' EXIT

execute() {
  echo "+ $*"
  if [[ "$DRY_RUN" -eq 0 ]]; then
    "$@"
  fi
}

copy_file() {
  local src="$1"
  local dst="$2"
  local mode="${3:-0644}"
  local dir
  dir="$(dirname "$dst")"

  if [[ ! -d "$dir" ]]; then
    execute mkdir -p "$dir"
  fi
  execute cp -f "$src" "$dst"
  execute chmod "$mode" "$dst"

  # Strip DESTDIR prefix when recording in manifest
  local record_path="$dst"
  if [[ -n "$DESTDIR" && "$record_path" == "${DESTDIR}"* ]]; then
    record_path="${record_path#"${DESTDIR}"}"
  fi
  echo "$record_path" >> "$TMP_MANIFEST"
}

copy_tree() {
  local src_dir="$1"
  local dst_dir="$2"

  if [[ ! -d "$src_dir" ]]; then
    return 0
  fi

  while IFS= read -r -d '' file; do
    local rel="${file#"${src_dir}/"}"
    local target="${dst_dir}/${rel}"
    copy_file "$file" "$target" 0644
  done < <(find "$src_dir" -type f -print0)
}

echo "=== Kynex Session Installation ==="
echo "Source:  $ROOT_DIR"
echo "DESTDIR: ${DESTDIR:-<root>}"
echo "Dry run: $DRY_RUN"

# 1. Desktop Session Entry
copy_file "$SCRIPT_DIR/kynex.desktop" "${DESTDIR}/usr/share/wayland-sessions/kynex.desktop" 0644

# 2. Session Startup Script
copy_file "$SCRIPT_DIR/startkynex" "${DESTDIR}/usr/local/bin/startkynex" 0755

# 3. Log Collection Script
copy_file "$SCRIPT_DIR/collect-logs.sh" "${DESTDIR}/usr/local/bin/kynex-collect-logs" 0755

# 4. Binaries (prefer release if built, fallback to debug)
DAEMON_BIN=""
SHELL_BIN=""

if [[ -x "$ROOT_DIR/target/release/kynex-daemon" ]]; then
  DAEMON_BIN="$ROOT_DIR/target/release/kynex-daemon"
elif [[ -x "$ROOT_DIR/target/debug/kynex-daemon" ]]; then
  DAEMON_BIN="$ROOT_DIR/target/debug/kynex-daemon"
fi

if [[ -x "$ROOT_DIR/target/release/kynex-shell" ]]; then
  SHELL_BIN="$ROOT_DIR/target/release/kynex-shell"
elif [[ -x "$ROOT_DIR/target/debug/kynex-shell" ]]; then
  SHELL_BIN="$ROOT_DIR/target/debug/kynex-shell"
fi

if [[ -n "$DAEMON_BIN" ]]; then
  copy_file "$DAEMON_BIN" "${DESTDIR}/usr/local/bin/kynex-daemon" 0755
else
  echo "Notice: kynex-daemon binary not built yet (run cargo build --release)" >&2
fi

if [[ -n "$SHELL_BIN" ]]; then
  copy_file "$SHELL_BIN" "${DESTDIR}/usr/local/bin/kynex-shell" 0755
else
  echo "Notice: kynex-shell binary not built yet (run cargo build --release)" >&2
fi

# 5. Systemd User Units
copy_file "$SCRIPT_DIR/kynex-session.target" "${DESTDIR}/usr/lib/systemd/user/kynex-session.target" 0644
copy_file "$SCRIPT_DIR/kynex-daemon.service" "${DESTDIR}/usr/lib/systemd/user/kynex-daemon.service" 0644
copy_file "$SCRIPT_DIR/kynex-shell.service" "${DESTDIR}/usr/lib/systemd/user/kynex-shell.service" 0644

# 6. Portal Configuration
copy_file "$SCRIPT_DIR/kynex-portals.conf" "${DESTDIR}/usr/share/xdg-desktop-portal/kynex-portals.conf" 0644

# 7. Style Pack Assets
copy_tree "$ROOT_DIR/styles/default" "${DESTDIR}/usr/share/kynex/styles/default"

# 8. QML Module Files
copy_tree "$ROOT_DIR/crates/kynex-shell/qml" "${DESTDIR}/usr/share/kynex/qml"

# 9. KWin Tracking & Shortcut Scripts
copy_tree "$ROOT_DIR/kwin-scripts/kynex-windows" "${DESTDIR}/usr/share/kwin/scripts/kynex-windows"
copy_tree "$ROOT_DIR/kwin-scripts/kynex-launcher" "${DESTDIR}/usr/share/kwin/scripts/kynex-launcher"

# 10. Record Manifest
echo "+ Recording manifest in $MANIFEST_FILE"
if [[ "$DRY_RUN" -eq 0 ]]; then
  mkdir -p "$MANIFEST_DIR"
  sort -u "$TMP_MANIFEST" > "$MANIFEST_FILE"
  echo "Installed $(wc -l < "$MANIFEST_FILE") files according to manifest."
else
  echo "[dry-run] Would write $(sort -u "$TMP_MANIFEST" | wc -l) lines to $MANIFEST_FILE"
fi

echo "Installation complete."
