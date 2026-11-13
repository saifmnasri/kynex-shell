#!/usr/bin/env bash
# Run the QML test suite 20 times under heavy CPU load (one yes per core)
# to detect timing races, rigid timers, and UI synchronization flakes (AUD-P2-02).
set -euo pipefail

cd "$(dirname "$0")/.."

CORES="$(nproc)"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do
        kill -9 "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "Starting CPU load ($CORES cores)..."
for ((i=0; i<CORES; i++)); do
    yes > /dev/null &
    PIDS+=($!)
done

# Ensure test binary is up to date
./scripts/test-qml-controls.sh >/dev/null 2>&1

PASS_COUNT=0
FAIL_COUNT=0

echo "Running QML suite 20 times under CPU load..."
for i in {1..20}; do
    if ./scripts/test-qml-controls.sh >/dev/null 2>&1; then
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
done

cleanup
echo "QML stress run results: $PASS_COUNT/20 passed, $FAIL_COUNT/20 failed"
if [[ $FAIL_COUNT -gt 0 ]]; then
    exit 1
fi
exit 0
