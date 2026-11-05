#pragma once

#include "rust/cxx.h"
#include <cstdint>

void kynex_register_core_types();

void kynex_set_style_tokens(
    rust::Str surface,
    rust::Str surface_raised,
    rust::Str text,
    rust::Str text_muted,
    rust::Str accent,
    rust::Str danger,
    rust::Str success,
    uint32_t spacing_unit,
    uint32_t radius_small,
    uint32_t radius_medium,
    uint32_t radius_large,
    uint32_t motion_fast_ms,
    uint32_t motion_normal_ms,
    uint32_t motion_slow_ms,
    rust::Str font_family,
    rust::Str font_fallback,
    uint32_t font_base_px
);
