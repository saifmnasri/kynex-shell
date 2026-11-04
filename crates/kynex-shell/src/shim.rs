//! C++ FFI bridge for `LayerSurface` and Qt core types.

/// CXX bridge declarations for the C++ shim.
#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cpp/shim.h");
        /// Registers `LayerSurface` in Qt QML.
        fn kynex_register_core_types();
    }
}

/// Registers the C++ `LayerSurface` QML type in `org.kynexos.core`.
pub fn register_types() {
    ffi::kynex_register_core_types();
}
