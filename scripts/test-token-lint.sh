#!/usr/bin/env bash
# Unit test for the style tokens linter in verify.sh.
set -euo pipefail

TMP_DIR="$(mktemp -d /tmp/kynex-token-lint-test.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

check_qml_tokens() {
  local target_dir="$1"
  local violations=0
  while IFS= read -r file; do
    if grep -En '#[0-9a-fA-F]{3,8}\b' "$file" | grep -v '^[[:space:]]*//' >/dev/null 2>&1; then
      violations=$((violations + 1))
    fi
    if grep -En '(font\.)?pixelSize[[:space:]]*:[[:space:]]*[0-9]+' "$file" | grep -v '^[[:space:]]*//' >/dev/null 2>&1; then
      violations=$((violations + 1))
    fi
  done < <(find "$target_dir" -name '*.qml' 2>/dev/null)
  return "$violations"
}

# Test 1: Valid QML with token bindings must pass
mkdir -p "$TMP_DIR/valid"
cat << 'EOF' > "$TMP_DIR/valid/Test.qml"
import QtQuick
import Kynex.Style

Rectangle {
    color: Style.colors.surface
    Text {
        font.pixelSize: Style.typography.basePx
        color: Style.colors.text
    }
}
EOF
if ! check_qml_tokens "$TMP_DIR/valid"; then
  echo "FAIL: Valid token bindings incorrectly rejected" >&2
  exit 1
fi

# Test 2: QML with hex color literal must fail
mkdir -p "$TMP_DIR/bad_color"
cat << 'EOF' > "$TMP_DIR/bad_color/Test.qml"
import QtQuick
Rectangle {
    color: "#1d2029"
}
EOF
if check_qml_tokens "$TMP_DIR/bad_color"; then
  echo "FAIL: Hex color literal was not rejected" >&2
  exit 1
fi

# Test 3: QML with hardcoded pixel size must fail
mkdir -p "$TMP_DIR/bad_pixelsize"
cat << 'EOF' > "$TMP_DIR/bad_pixelsize/Test.qml"
import QtQuick
Text {
    font.pixelSize: 14
}
EOF
if check_qml_tokens "$TMP_DIR/bad_pixelsize"; then
  echo "FAIL: Hard-coded font.pixelSize was not rejected" >&2
  exit 1
fi

# Test 4: QML with hardcoded pixelSize without font. prefix must fail
mkdir -p "$TMP_DIR/bad_pixelsize2"
cat << 'EOF' > "$TMP_DIR/bad_pixelsize2/Test.qml"
import QtQuick
Item {
    pixelSize: 16
}
EOF
if check_qml_tokens "$TMP_DIR/bad_pixelsize2"; then
  echo "FAIL: Hard-coded pixelSize was not rejected" >&2
  exit 1
fi

echo "All style token linter tests passed!"
exit 0
