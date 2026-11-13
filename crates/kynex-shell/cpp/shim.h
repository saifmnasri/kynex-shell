#pragma once

#include "rust/cxx.h"
#include <QtQml/QQmlApplicationEngine>
#include <cstdint>

void kynex_register_core_types();
void kynex_load_bundled_fonts();
void kynex_init_security_limits();
void kynex_register_image_provider(QQmlApplicationEngine &engine);
bool kynex_is_svg_safe_slice(rust::Slice<const uint8_t> data);
bool kynex_test_decode_icon(rust::Str path, int32_t width, int32_t height, int32_t scale);
bool kynex_test_real_tray_menu_model_open(rust::Str qml_path);

void kynex_set_style_tokens(
    rust::Str surface,
    rust::Str surface_raised,
    rust::Str text,
    rust::Str text_muted,
    rust::Str accent,
    rust::Str danger,
    rust::Str warning,
    rust::Str success,
    rust::Str focus_ring,
    uint32_t spacing_unit,
    uint32_t spacing_small,
    uint32_t spacing_medium,
    uint32_t spacing_large,
    uint32_t spacing_panel,
    uint32_t radius_small,
    uint32_t radius_medium,
    uint32_t radius_large,
    uint32_t motion_fast_ms,
    uint32_t motion_normal_ms,
    uint32_t motion_slow_ms,
    rust::Str font_family,
    rust::Str font_fallback,
    uint32_t font_base_px,
    double opacity_disabled,
    double opacity_hover,
    double opacity_pressed,
    double opacity_focus
);

uint64_t kynex_steady_clock_ns();
