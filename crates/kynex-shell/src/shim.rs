//! C++ FFI bridge for `LayerSurface` and Qt core types.

/// CXX bridge declarations for the C++ shim.
#[cxx::bridge]
pub mod ffi {
    // SAFETY: Declares the C++ FFI function `kynex_register_core_types` implemented
    // in `cpp/shim.cpp`. It interacts solely with the Qt QML type registration system
    // (`qmlRegisterType`), does not dereference raw pointers, and has no side effects
    // violating Rust safety invariants.
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
