#!/usr/bin/env bash
# Unit test for the style tokens linter in verify.sh.
set -euo pipefail

TMP_DIR="$(mktemp -d /tmp/kynex-token-lint-test.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

check_qml_tokens() {
  local target_dir="$1"
  ./scripts/check-style-tokens.py "$target_dir" >/dev/null 2>&1
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

# Test 15: Attack test - comparison 'percentage < 20' must fail
mkdir -p "$TMP_DIR/attack_percentage_20"
cat << 'EOF' > "$TMP_DIR/attack_percentage_20/Test.qml"
import QtQuick
Text {
    color: (percentage < 20) ? "orange" : "green"
}
EOF
if check_qml_tokens "$TMP_DIR/attack_percentage_20"; then
  echo "FAIL: Attack test 'percentage < 20' was not rejected" >&2
  exit 1
fi

# Test 16: Attack test - comparison 'x > 5' must fail
mkdir -p "$TMP_DIR/attack_x_gt_5"
cat << 'EOF' > "$TMP_DIR/attack_x_gt_5/Test.qml"
import QtQuick
Item {
    visible: x > 5
}
EOF
if check_qml_tokens "$TMP_DIR/attack_x_gt_5"; then
  echo "FAIL: Attack test 'x > 5' was not rejected" >&2
  exit 1
fi

# Test 17: Attack test - bare float literal 'opacity: 0.5' must fail
mkdir -p "$TMP_DIR/attack_opacity_05"
cat << 'EOF' > "$TMP_DIR/attack_opacity_05/Test.qml"
import QtQuick
Rectangle {
    opacity: 0.5
}
EOF
if check_qml_tokens "$TMP_DIR/attack_opacity_05"; then
  echo "FAIL: Attack test 'opacity: 0.5' was not rejected" >&2
  exit 1
fi

# Test 18: Attack test - JS function call 'Math.max(3, a)' must fail
mkdir -p "$TMP_DIR/attack_math_max"
cat << 'EOF' > "$TMP_DIR/attack_math_max/Test.qml"
import QtQuick
Item {
    scale: Math.max(3, a)
}
EOF
if check_qml_tokens "$TMP_DIR/attack_math_max"; then
  echo "FAIL: Attack test 'Math.max(3, a)' was not rejected" >&2
  exit 1
fi

# Test 19: Attack test - property declaration 'property int n: 7' must fail
mkdir -p "$TMP_DIR/attack_property_int_7"
cat << 'EOF' > "$TMP_DIR/attack_property_int_7/Test.qml"
import QtQuick
Item {
    property int n: 7
}
EOF
if check_qml_tokens "$TMP_DIR/attack_property_int_7"; then
  echo "FAIL: Attack test 'property int n: 7' was not rejected" >&2
  exit 1
fi

echo "All style token linter attack tests passed (19/19 cases verified)!"
exit 0
