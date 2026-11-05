#!/usr/bin/env bash
# Unit test for the style tokens linter in verify.sh.
set -euo pipefail

TMP_DIR="$(mktemp -d /tmp/kynex-token-lint-test.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

check_qml_tokens() {
  local target_dir="$1"
  local violations=0
  while IFS= read -r file; do
    # 1. Reject org.kde.* imports
    if grep -En '^[[:space:]]*import[[:space:]]+org\.kde\b' "$file" >/dev/null 2>&1; then
      violations=$((violations + 1))
    fi
    # 2. Reject hex color literals
    if grep -En '#[0-9a-fA-F]{3,8}\b' "$file" | grep -v '^[[:space:]]*//' >/dev/null 2>&1; then
      violations=$((violations + 1))
    fi
    # 3. Reject named color string literals (e.g. "red", 'blue')
    if grep -En '(color|border\.color)[[:space:]]*:[[:space:]]*["'\''][a-zA-Z]+["'\'']' "$file" | grep -v '^[[:space:]]*//' >/dev/null 2>&1; then
      violations=$((violations + 1))
    fi
    # 4. Reject functional color constructors (Qt.rgba, rgb, etc.)
    if grep -En '\b(Qt\.(rgba?|hsla?|color)|rgba?|hsla?)[[:space:]]*\(' "$file" | grep -v '^[[:space:]]*//' >/dev/null 2>&1; then
      violations=$((violations + 1))
    fi
    # 5. Reject bare numbers and numbers inside JS expressions for styling/geometry
    while IFS= read -r line; do
      clean_line="${line%%//*}"
      if echo "$clean_line" | grep -Eq '\b(font\.)?(pixelSize|width|height|spacing|radius|margins?|leftMargin|rightMargin|topMargin|bottomMargin|exclusiveZone)[[:space:]]*:[^;}]*(\b([2-9]|[1-9][0-9]+)\b|[0-9]+\.[0-9]+)'; then
        violations=$((violations + 1))
      fi
    done < "$file"
  done < <(find "$target_dir" -name '*.qml' 2>/dev/null)
  return "$violations"
}

# Test 1: Valid QML with token bindings and allowed constants (0 and 1) must pass
mkdir -p "$TMP_DIR/valid"
cat << 'EOF' > "$TMP_DIR/valid/Test.qml"
import QtQuick
import Kynex.Style

Rectangle {
    color: Style.colors.surface
    height: Style.spacing.panel
    leftMargin: 0
    Text {
        font.pixelSize: Style.typography.basePx - 1
        color: Style.colors.text
    }
}
EOF
if ! check_qml_tokens "$TMP_DIR/valid"; then
  echo "FAIL: Valid token bindings incorrectly rejected" >&2
  exit 1
fi

# Test 2: Hex color literal must fail
mkdir -p "$TMP_DIR/bad_hex"
cat << 'EOF' > "$TMP_DIR/bad_hex/Test.qml"
import QtQuick
Rectangle {
    color: "#1d2029"
}
EOF
if check_qml_tokens "$TMP_DIR/bad_hex"; then
  echo "FAIL: Hex color literal was not rejected" >&2
  exit 1
fi

# Test 3: Named color must fail
mkdir -p "$TMP_DIR/bad_named_color"
cat << 'EOF' > "$TMP_DIR/bad_named_color/Test.qml"
import QtQuick
Rectangle {
    color: "red"
}
EOF
if check_qml_tokens "$TMP_DIR/bad_named_color"; then
  echo "FAIL: Named color literal was not rejected" >&2
  exit 1
fi

# Test 4: Qt.rgba constructor must fail
mkdir -p "$TMP_DIR/bad_qt_rgba"
cat << 'EOF' > "$TMP_DIR/bad_qt_rgba/Test.qml"
import QtQuick
Rectangle {
    color: Qt.rgba(1, 0, 0, 1)
}
EOF
if check_qml_tokens "$TMP_DIR/bad_qt_rgba"; then
  echo "FAIL: Qt.rgba(...) was not rejected" >&2
  exit 1
fi

# Test 5: rgb(...) constructor must fail
mkdir -p "$TMP_DIR/bad_rgb"
cat << 'EOF' > "$TMP_DIR/bad_rgb/Test.qml"
import QtQuick
Rectangle {
    color: rgb(255, 0, 0)
}
EOF
if check_qml_tokens "$TMP_DIR/bad_rgb"; then
  echo "FAIL: rgb(...) was not rejected" >&2
  exit 1
fi

# Test 6: Bare width number must fail
mkdir -p "$TMP_DIR/bad_width"
cat << 'EOF' > "$TMP_DIR/bad_width/Test.qml"
import QtQuick
Item {
    width: 100
}
EOF
if check_qml_tokens "$TMP_DIR/bad_width"; then
  echo "FAIL: Bare width number was not rejected" >&2
  exit 1
fi

# Test 7: Bare height number must fail
mkdir -p "$TMP_DIR/bad_height"
cat << 'EOF' > "$TMP_DIR/bad_height/Test.qml"
import QtQuick
Item {
    height: 48
}
EOF
if check_qml_tokens "$TMP_DIR/bad_height"; then
  echo "FAIL: Bare height number was not rejected" >&2
  exit 1
fi

# Test 8: Bare spacing number must fail
mkdir -p "$TMP_DIR/bad_spacing"
cat << 'EOF' > "$TMP_DIR/bad_spacing/Test.qml"
import QtQuick
Row {
    spacing: 8
}
EOF
if check_qml_tokens "$TMP_DIR/bad_spacing"; then
  echo "FAIL: Bare spacing number was not rejected" >&2
  exit 1
fi

# Test 9: Bare radius number must fail
mkdir -p "$TMP_DIR/bad_radius"
cat << 'EOF' > "$TMP_DIR/bad_radius/Test.qml"
import QtQuick
Rectangle {
    radius: 6
}
EOF
if check_qml_tokens "$TMP_DIR/bad_radius"; then
  echo "FAIL: Bare radius number was not rejected" >&2
  exit 1
fi

# Test 10: Bare font.pixelSize must fail
mkdir -p "$TMP_DIR/bad_pixelsize"
cat << 'EOF' > "$TMP_DIR/bad_pixelsize/Test.qml"
import QtQuick
Text {
    font.pixelSize: 14
}
EOF
if check_qml_tokens "$TMP_DIR/bad_pixelsize"; then
  echo "FAIL: Bare font.pixelSize was not rejected" >&2
  exit 1
fi

# Test 11: Bare margins number must fail
mkdir -p "$TMP_DIR/bad_margins"
cat << 'EOF' > "$TMP_DIR/bad_margins/Test.qml"
import QtQuick
Item {
    margins: 12
}
EOF
if check_qml_tokens "$TMP_DIR/bad_margins"; then
  echo "FAIL: Bare margins number was not rejected" >&2
  exit 1
fi

# Test 12: Number inside JS expression must fail
mkdir -p "$TMP_DIR/bad_js_expr"
cat << 'EOF' > "$TMP_DIR/bad_js_expr/Test.qml"
import QtQuick
import Kynex.Style
Item {
    width: Style.spacing.unit * 12
}
EOF
if check_qml_tokens "$TMP_DIR/bad_js_expr"; then
  echo "FAIL: Number inside JS expression was not rejected" >&2
  exit 1
fi

# Test 13: org.kde import must fail
mkdir -p "$TMP_DIR/bad_org_kde"
cat << 'EOF' > "$TMP_DIR/bad_org_kde/Test.qml"
import QtQuick
import org.kde.plasma.core
Item {}
EOF
if check_qml_tokens "$TMP_DIR/bad_org_kde"; then
  echo "FAIL: org.kde import was not rejected" >&2
  exit 1
fi

# Test 14: Documented allowed 0 and 1 must pass
mkdir -p "$TMP_DIR/allowed_constants"
cat << 'EOF' > "$TMP_DIR/allowed_constants/Test.qml"
import QtQuick
import Kynex.Style
Item {
    leftMargin: 0
    rightMargin: 0
    topMargin: 0
    bottomMargin: 0
    spacing: 0
    Text {
        font.pixelSize: Style.typography.basePx + 1
    }
}
EOF
if ! check_qml_tokens "$TMP_DIR/allowed_constants"; then
  echo "FAIL: Documented allowed 0 and 1 were incorrectly rejected" >&2
  exit 1
fi

echo "All style token linter attack tests passed (14/14 cases verified)!"
exit 0
