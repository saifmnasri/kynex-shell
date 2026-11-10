#!/usr/bin/env bash
# Record command execution as raw evidence for work reports.
# Usage: scripts/evidence.sh <id> -- <command...>
set -euo pipefail

if [[ $# -lt 3 || "$2" != "--" ]]; then
  echo "Usage: $0 <id> -- <command...>" >&2
  exit 2
fi

ID="$1"
shift 2

EVIDENCE_DIR="docs/work/evidence"
mkdir -p "$EVIDENCE_DIR"
OUT_FILE="$EVIDENCE_DIR/${ID}.txt"

TMP_OUT="$(mktemp -p "$EVIDENCE_DIR")"
TIMESTAMP="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

# Execute command capturing stdout and stderr combined
set +e
"$@" > "$TMP_OUT" 2>&1
EXIT_CODE=$?
set -e

{
  echo "=== COMMAND ==="
  echo "$*"
  echo "=== TIMESTAMP: $TIMESTAMP ==="
  echo "=== EXIT CODE: $EXIT_CODE ==="
  echo "=== RAW OUTPUT ==="
  cat "$TMP_OUT"
} > "$OUT_FILE"

cat "$TMP_OUT"
rm -f "$TMP_OUT"

exit "$EXIT_CODE"
