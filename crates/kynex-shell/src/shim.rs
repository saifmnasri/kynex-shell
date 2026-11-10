//! C++ FFI bridge for `LayerSurface`, Qt core types, and `Kynex.Style` tokens.

use core::pin::Pin;
use kynex_style::TokensManifest;

/// CXX bridge declarations for the C++ shim.
#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        /// Type alias for `QQmlApplicationEngine` bridging into C++.
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;

        include!("cpp/shim.h");
        /// Registers `LayerSurface` in `Kynex.Shell` and `Style` in `Kynex.Style`.
        fn kynex_register_core_types();

        /// Loads bundled fonts into QFontDatabase.
        fn kynex_load_bundled_fonts();

        /// Initializes global Qt security limits (16 MB allocation limit).
        fn kynex_init_security_limits();

        /// Registers KynexIconProvider into the QML engine.
        fn kynex_register_image_provider(engine: Pin<&mut QQmlApplicationEngine>);

        /// Validates SVG security against external DTDs/entities/URIs and use-bombs.
        #[must_use]
        fn kynex_is_svg_safe_slice(data: &[u8]) -> bool;

        /// Decodes an icon through Qt enforcing dimensions and allocation limit.
        #[must_use]
        fn kynex_test_decode_icon(path: &str, width: i32, height: i32, scale: i32) -> bool;

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
            focus_ring: &str,
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
            opacity_disabled: f64,
            opacity_hover: f64,
            opacity_pressed: f64,
            opacity_focus: f64,
        );
    }
}

/// Registers the C++ `LayerSurface` QML type in `Kynex.Shell` and `Style` singleton in `Kynex.Style`.
pub fn register_types() {
    ffi::kynex_register_core_types();
}

/// Loads bundled application fonts into the Qt font database.
pub fn load_bundled_fonts() {
    ffi::kynex_load_bundled_fonts();
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
        &dark.focus_ring,
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
        tokens.opacity.disabled,
        tokens.opacity.hover,
        tokens.opacity.pressed,
        tokens.opacity.focus,
    );
}

/// Initializes Qt-wide security limits (16 MB allocation limit).
pub fn init_security_limits() {
    ffi::kynex_init_security_limits();
}

/// Registers the async icon provider (`image://kynex-icon/...`) on the QML application engine.
pub fn register_image_provider(engine: Pin<&mut cxx_qt_lib::QQmlApplicationEngine>) {
    ffi::kynex_register_image_provider(engine);
}

/// Validates whether an SVG's raw XML content is safe to parse and render.
#[must_use]
pub fn is_svg_safe(data: &[u8]) -> bool {
    ffi::kynex_is_svg_safe_slice(data)
}

/// Tests decoding an icon file through the C++ decoder with security bounds.
#[must_use]
pub fn test_decode_icon(path: &str, width: i32, height: i32, scale: i32) -> bool {
    ffi::kynex_test_decode_icon(path, width, height, scale)
}

/// Resolves an icon name to an absolute file path using `kynex_icons::lookup_icon`.
///
/// # Safety
/// `name` must be a valid null-terminated C string.
/// `out_buf` must point to a writable buffer of at least `buf_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn kynex_resolve_icon(
    name: *const std::ffi::c_char,
    size: u32,
    scale: u32,
    out_buf: *mut std::ffi::c_char,
    buf_size: usize,
) -> bool {
    if name.is_null() || out_buf.is_null() || buf_size == 0 {
        return false;
    }

    let Ok(c_str) = std::ffi::CStr::from_ptr(name).to_str() else {
        return false;
    };

    match kynex_icons::lookup_icon(c_str, size, scale, None) {
        Ok(Some(path)) => {
            let path_bytes = path.as_os_str().as_encoded_bytes();
            if path_bytes.len() + 1 > buf_size {
                return false;
            }
            std::ptr::copy_nonoverlapping(
                path_bytes.as_ptr().cast::<std::ffi::c_char>(),
                out_buf,
                path_bytes.len(),
            );
            *out_buf.add(path_bytes.len()) = 0;
            true
        }
        _ => false,
    }
}
