//! C++ FFI bridge for `LayerSurface`, Qt core types, and `Kynex.Style` tokens.

use kynex_style::TokensManifest;

/// CXX bridge declarations for the C++ shim.
#[cxx::bridge]
pub mod ffi {
    // SAFETY: Declares the C++ FFI functions `kynex_register_core_types` and
    // `kynex_set_style_tokens` implemented in `cpp/shim.cpp`.
    // - `kynex_register_core_types` interacts solely with the Qt QML type registration system
    //   (`qmlRegisterType` and `qmlRegisterSingletonInstance`), does not dereference raw pointers,
    //   and has no side effects violating Rust safety invariants.
    // - `kynex_set_style_tokens` receives validated Rust string slices and integers, converting
    //   them to Qt values on the C++ Style singleton. String views are borrowed only during
    //   the call, preserving Rust lifetime and memory safety guarantees.
    unsafe extern "C++" {
        include!("cpp/shim.h");
        /// Registers `LayerSurface` in `Kynex.Shell` and `Style` in `Kynex.Style`.
        fn kynex_register_core_types();

        /// Updates the `Style` singleton with parsed design tokens.
        #[allow(clippy::too_many_arguments)]
        fn kynex_set_style_tokens(
            surface: &str,
            surface_raised: &str,
            text: &str,
            text_muted: &str,
            accent: &str,
            danger: &str,
            warning: &str,
            success: &str,
            spacing_unit: u32,
            spacing_small: u32,
            spacing_medium: u32,
            spacing_large: u32,
            spacing_panel: u32,
            radius_small: u32,
            radius_medium: u32,
            radius_large: u32,
            motion_fast_ms: u32,
            motion_normal_ms: u32,
            motion_slow_ms: u32,
            font_family: &str,
            font_fallback: &str,
            font_base_px: u32,
        );
    }
}

/// Registers the C++ `LayerSurface` QML type in `Kynex.Shell` and `Style` singleton in `Kynex.Style`.
pub fn register_types() {
    ffi::kynex_register_core_types();
}

/// Initializes the C++ `Style` singleton from a parsed [`TokensManifest`].
pub fn init_tokens(tokens: &TokensManifest) {
    let dark = &tokens.color.dark;
    ffi::kynex_set_style_tokens(
        &dark.surface,
        &dark.surface_raised,
        &dark.text,
        &dark.text_muted,
        &dark.accent,
        &dark.danger,
        &dark.warning,
        &dark.success,
        tokens.spacing.unit,
        tokens.spacing.small,
        tokens.spacing.medium,
        tokens.spacing.large,
        tokens.spacing.panel,
        tokens.radius.small,
        tokens.radius.medium,
        tokens.radius.large,
        tokens.motion.fast_ms,
        tokens.motion.normal_ms,
        tokens.motion.slow_ms,
        &tokens.typography.family,
        &tokens.typography.fallback,
        tokens.typography.base_px,
    );
}
