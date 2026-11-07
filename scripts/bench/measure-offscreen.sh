#!/usr/bin/env bash
# Runs kynex-shell offscreen with software rendering 3 times for 60 s each.
# Measures idle PSS/RSS and reports the median and spread to eliminate VM compositor jitter.
set -euo pipefail
cd "$(dirname "$0")/../.."

ROOT_DIR="$(pwd)"
BENCH_DIR="$ROOT_DIR/dev/bench"
mkdir -p "$BENCH_DIR"

DURATION="${1:-60}"
BUILD_PROFILE="${KYNEX_BUILD_PROFILE:-release}"

echo "=== Building kynex-shell ($BUILD_PROFILE) ==="
CARGO_ARGS=("--bin" "kynex-shell" "--bin" "kynex-daemon")
if [[ "$BUILD_PROFILE" == "release" ]]; then
  CARGO_ARGS+=("--release" "--locked")
fi
cargo build "${CARGO_ARGS[@]}"

BIN="$ROOT_DIR/target/$BUILD_PROFILE/kynex-shell"
MEASURE="$ROOT_DIR/scripts/bench/measure.py"

RUN_FILES=()
PSS_VALUES=()

export QT_QPA_PLATFORM=offscreen
export QT_QUICK_BACKEND=software
export KYNEX_STYLE_DIR="$ROOT_DIR/styles/default"

echo "=== Starting 3 Offscreen Measurement Runs (${DURATION}s each) ==="

for run in 1 2 3; do
  echo "--- Run $run of 3 ---"
  
  # Start offscreen shell in background
  "$BIN" >/dev/null 2>&1 &
  SHELL_PID=$!
  
  # Give shell 1 second to initialize
  sleep 1
  
  RUN_LABEL="offscreen-${BUILD_PROFILE}-run${run}"
  python3 "$MEASURE" --label "$RUN_LABEL" --procs kynex-shell --duration "$DURATION"
  
  # Clean up process
  kill -TERM "$SHELL_PID" 2>/dev/null || true
  wait "$SHELL_PID" 2>/dev/null || true
  
  # Find generated JSON
  mapfile -t MATCHED_JSONS < <(find "$BENCH_DIR" -maxdepth 1 -name "${RUN_LABEL}-*.json" -printf '%T@ %p\n' | sort -rn)
  if [[ ${#MATCHED_JSONS[@]} -eq 0 ]]; then
    echo "ERROR: Could not find benchmark output for run $run" >&2
    exit 1
  fi
  LATEST_JSON="${MATCHED_JSONS[0]#* }"
  RUN_FILES+=("$LATEST_JSON")
  
  # Extract PSS avg
  PSS=$(python3 -c "import json; d=json.load(open('$LATEST_JSON')); print(d['procs']['kynex-shell']['pss_mb_avg'])")
  PSS_VALUES+=("$PSS")
  echo "Run $run PSS: ${PSS} MB"
done

# Compute statistics in Python
python3 -c "
import json, statistics

pss = [float(x) for x in '''${PSS_VALUES[*]}'''.split()]
runs = '''${RUN_FILES[*]}'''.split()

median_pss = statistics.median(pss)
min_pss = min(pss)
max_pss = max(pss)
spread = max_pss - min_pss

print('\n================ Offscreen Benchmark Summary ================')
for i, (val, path) in enumerate(zip(pss, runs), 1):
    print(f'  Run {i}: {val:6.2f} MB PSS  ({path})')
print('------------------------------------------------------------')
print(f'  Median PSS : {median_pss:6.2f} MB')
print(f'  Min PSS    : {min_pss:6.2f} MB')
print(f'  Max PSS    : {max_pss:6.2f} MB')
print(f'  Spread     : {spread:6.2f} MB ({(spread/median_pss*100.0 if median_pss>0 else 0):.2f}%)')
print('============================================================')

# Write median summary JSON for compare.py
median_idx = pss.index(median_pss)
median_file = runs[median_idx]
print(f'Representative median JSON for compare.py: {median_file}')
"
