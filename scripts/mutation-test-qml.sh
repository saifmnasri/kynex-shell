#!/usr/bin/env bash
# Mutation testing harness for Kynex QML base controls and visual regression tests.
# Applies 6 adversarial mutations and asserts that the test suite catches (kills) every single one.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Adversarial QML Mutation Testing ==="

FAILED_MUTATIONS=0

run_mutation() {
  local id="$1"
  local desc="$2"
  local target_file="$3"
  local find_str="$4"
  local replace_str="$5"
  local test_cmd="$6"

  echo "------------------------------------------------------------"
  echo "Mutation #$id: $desc"
  echo "Target: $target_file"

  if [[ ! -f "$target_file" ]]; then
    echo "ERROR: File $target_file not found!" >&2
    exit 1
  fi

  # Backup original file
  cp "$target_file" "${target_file}.mutation_bak"
  
  # Ensure cleanup on any unexpected exit
  trap '[[ -f "'"${target_file}.mutation_bak"'" ]] && mv -f "'"${target_file}.mutation_bak"'" "'"$target_file"'"; trap - EXIT INT TERM' EXIT INT TERM

  # Apply mutation
  python3 -c "
import sys
with open('$target_file', 'r', encoding='utf-8') as f:
    content = f.read()
if '''$find_str''' not in content:
    sys.stderr.write('Pattern not found in file: $find_str\n')
    sys.exit(1)
mutated = content.replace('''$find_str''', '''$replace_str''', 1)
with open('$target_file', 'w', encoding='utf-8') as f:
    f.write(mutated)
"

  # Run test command (expecting failure)
  local test_status=0
  local test_out=""
  set +e
  test_out="$($test_cmd 2>&1)"
  test_status=$?
  set -e

  # Restore original file immediately
  mv -f "${target_file}.mutation_bak" "$target_file"
  trap - EXIT INT TERM

  # If target is C++, trigger touch/rebuild so future runs aren't stale
  if [[ "$target_file" == *.cpp ]]; then
    touch "$target_file"
  fi

  if [[ $test_status -ne 0 ]]; then
    echo "Result: KILLED (Test failed with exit code $test_status as expected)"
  else
    echo "Result: SURVIVED (CRITICAL FLAW: Test passed unexpectedly!)"
    echo "$test_out"
    FAILED_MUTATIONS=$((FAILED_MUTATIONS + 1))
  fi
}

# 1. Remove focus ring from KynexButton
run_mutation 1 "Remove active focus ring from KynexButton" \
  "crates/kynex-shell/qml/Kynex/Controls/KynexButton.qml" \
  "border.color: root.activeFocus ? Style.colors.focusRing : Style.colors.surfaceRaised" \
  "border.color: Style.colors.surfaceRaised" \
  "./scripts/test-qml-controls.sh"

# 2. Break Switch keyboard activation
run_mutation 2 "Break keyboard activation toggle in KynexSwitch" \
  "crates/kynex-shell/qml/Kynex/Controls/KynexSwitch.qml" \
  "function toggle(): void {" \
  "function toggle(): void { return;" \
  "./scripts/test-qml-controls.sh"

# 3. Make Slider ignore disabled
run_mutation 3 "Make KynexSlider ignore disabled attribute" \
  "crates/kynex-shell/qml/Kynex/Controls/KynexSlider.qml" \
  "if (root.enabled) {" \
  "if (true) {" \
  "./scripts/test-qml-controls.sh"

# 4. Change a token color
run_mutation 4 "Change token accent color to hostile red" \
  "tests/visual/capture.cpp" \
  '"#6aa6ff", "#ff6b6b"' \
  '"#ff0000", "#ff6b6b"' \
  "./scripts/visual-test.sh"

# 5. Break RTL mirroring
run_mutation 5 "Break RTL layout mirroring in KynexSwitch" \
  "crates/kynex-shell/qml/Kynex/Controls/KynexSwitch.qml" \
  "LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft" \
  "LayoutMirroring.enabled: false" \
  "./scripts/visual-test.sh"

# 6. Remove Accessible.name
run_mutation 6 "Remove Accessible.name from KynexButton" \
  "crates/kynex-shell/qml/Kynex/Controls/KynexButton.qml" \
  "Accessible.name: text" \
  'Accessible.name: ""' \
  "./scripts/test-qml-controls.sh"

# Re-run visual-test once to make sure capture binary is in clean baseline state
./scripts/visual-test.sh >/dev/null 2>&1 || true

echo "------------------------------------------------------------"
if [[ $FAILED_MUTATIONS -eq 0 ]]; then
  echo "SUCCESS: All 6 adversarial mutations were successfully KILLED by the test suite."
  exit 0
else
  echo "FAIL: $FAILED_MUTATIONS mutation(s) survived undetected!"
  exit 1
fi
