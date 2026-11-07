#!/usr/bin/env bash
# Deterministic Qt Quick Test runner for Kynex base controls.
set -euo pipefail

cd "$(dirname "$0")/.."

BUILD_DIR="tests/qml_tests"
RUNNER="$BUILD_DIR/controls_test_runner"

# Compile if needed
if [[ ! -x "$RUNNER" || "$BUILD_DIR/main.cpp" -nt "$RUNNER" || "crates/kynex-shell/cpp/style.cpp" -nt "$RUNNER" || "crates/kynex-shell/cpp/style.h" -nt "$RUNNER" ]]; then
    mkdir -p "$BUILD_DIR"
    /usr/lib/qt6/libexec/moc "$BUILD_DIR/main.cpp" -o "$BUILD_DIR/main.moc"
    /usr/lib/qt6/libexec/moc crates/kynex-shell/cpp/style.h -o "$BUILD_DIR/style.moc.cpp"
    # shellcheck disable=SC2046
    g++ -std=c++17 -fPIC "$BUILD_DIR/main.cpp" "$BUILD_DIR/style.moc.cpp" crates/kynex-shell/cpp/style.cpp \
        $(pkg-config --cflags --libs Qt6QuickTest Qt6Quick Qt6Qml Qt6Gui Qt6Core) \
        -o "$RUNNER"
fi

export QT_QPA_PLATFORM=offscreen
export QT_QUICK_BACKEND=software
"$RUNNER" -input "$BUILD_DIR/tst_controls.qml"

