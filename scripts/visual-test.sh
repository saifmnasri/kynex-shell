#!/usr/bin/env bash
# Visual regression testing for Kynex Gallery components.
# Compares rendered components in dark/light, 100%/200% scale, and RTL against golden images.
# Usage: ./scripts/visual-test.sh [--update]
set -euo pipefail

cd "$(dirname "$0")/.."

UPDATE=0
if [[ "${1:-}" == "--update" ]]; then
    UPDATE=1
fi

BUILD_DIR="tests/visual"
CAPTURE="$BUILD_DIR/capture"

# Build capture binary if needed
if [[ ! -x "$CAPTURE" || "$BUILD_DIR/capture.cpp" -nt "$CAPTURE" || "crates/kynex-shell/cpp/style.cpp" -nt "$CAPTURE" || "crates/kynex-shell/cpp/style.h" -nt "$CAPTURE" ]]; then
    mkdir -p "$BUILD_DIR"
    mkdir -p "tests/qml_tests"
    /usr/lib/qt6/libexec/moc crates/kynex-shell/cpp/style.h -o "tests/qml_tests/style.moc.cpp"
    # shellcheck disable=SC2046
    g++ -std=c++17 -fPIC "$BUILD_DIR/capture.cpp" "tests/qml_tests/style.moc.cpp" crates/kynex-shell/cpp/style.cpp \
        $(pkg-config --cflags --libs Qt6Quick Qt6Qml Qt6Gui Qt6Core) \
        -o "$CAPTURE"
fi

export LC_ALL=C.UTF-8
export LANG=C.UTF-8
export QT_QPA_PLATFORM=offscreen
export QT_QUICK_BACKEND=software
FONTCONFIG_FILE="$(pwd)/tests/visual/minimal-fontconfig.conf"
export FONTCONFIG_FILE
QT_QPA_FONTDIR="$(pwd)/assets/fonts"
export QT_QPA_FONTDIR
mkdir -p /tmp/fc-cache

VARIANTS=("dark-100" "dark-200" "light-100" "light-200" "rtl-100" "launcher-results" "launcher-no-results" "launcher-empty" "launcher-rtl")
TEMP_DIR=$(mktemp -d /tmp/kynex-visual-XXXXXX)
trap 'rm -rf "$TEMP_DIR"' EXIT

# Documented tolerance: RMS difference threshold is set to 2.5 (on 0-255 channel scale).
# A threshold of 2.5 accommodates cross-architecture subpixel rasterization rounding
# (e.g. ARM64 NEON vs x86_64 AVX/SSE Mesa llvmpipe and FreeType anti-aliasing)
# while strictly detecting any geometric shift, missing component, or color regression (which produce RMS >= 10.0).
RMS_TOLERANCE="2.5"
FAILURES_DIR="tests/visual/failures"
mkdir -p "$FAILURES_DIR"

echo "=== Visual Regression Test (Tolerance: RMS <= ${RMS_TOLERANCE}) ==="

for variant in "${VARIANTS[@]}"; do
    scale=1
    if [[ "$variant" == *"-200" ]]; then
        scale=2
    fi

    golden_file="tests/visual/golden-${variant}.png"

    if ((UPDATE)); then
        echo "Updating golden image for $variant..."
        QT_SCALE_FACTOR="$scale" "$CAPTURE" "$variant" "$golden_file"
    else
        if [[ ! -f "$golden_file" ]]; then
            echo "ERROR: Missing golden image $golden_file. Run with --update to generate it." >&2
            exit 1
        fi

        render_file="$TEMP_DIR/${variant}.png"
        diff_file="$FAILURES_DIR/${variant}-diff.png"
        QT_SCALE_FACTOR="$scale" "$CAPTURE" "$variant" "$render_file" >/dev/null

        # Compare using native QImage RMS computation
        if ! cmp_output=$("$CAPTURE" compare "$golden_file" "$render_file" "$RMS_TOLERANCE" "$diff_file" 2>&1); then
            echo "$cmp_output" >&2
            cp "$render_file" "$FAILURES_DIR/${variant}-actual.png"
            cp "$golden_file" "$FAILURES_DIR/${variant}-golden.png"
            {
                echo "=== Visual Regression Failure Diagnostics ==="
                echo "Failed variant: $variant"
                echo "RMS output: $cmp_output"
                echo "Date: $(date -u)"
                echo
                echo "=== Fontconfig Diagnostics ==="
                echo "FONTCONFIG_FILE: ${FONTCONFIG_FILE:-<unset>}"
                echo "QT_QPA_FONTDIR: ${QT_QPA_FONTDIR:-<unset>}"
                echo "fc-match Inter: $(fc-match Inter 2>&1 || true)"
                echo "fc-match sans-serif: $(fc-match sans-serif 2>&1 || true)"
                echo
                echo "fc-list : family | grep -i inter:"
                fc-list : family 2>&1 | grep -i inter || echo "<none>"
                echo
                echo "fc-list (all families count): $(fc-list : family 2>&1 | sort -u | wc -l)"
                fc-list : family 2>&1 | sort -u || true
            } > "$FAILURES_DIR/font-diagnostics.txt"
            echo "FAIL: $variant exceeds RMS tolerance $RMS_TOLERANCE!" >&2
            exit 1
        else
            echo "PASS: $variant ($cmp_output)"
        fi
    fi
done

# If all variants passed, clean up failures directory
rm -rf "$FAILURES_DIR"

echo "Visual regression suite passed."
