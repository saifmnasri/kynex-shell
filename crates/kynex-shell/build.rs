//! Build script for `kynex-shell` linking Qt 6 and `LayerShellQt`.

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("Kynex.Shell"))
        .files([
            "src/clock.rs",
            "src/network.rs",
            "src/power.rs",
            "src/shim.rs",
        ])
        .cpp_files([
            "cpp/layersurface.cpp",
            "cpp/layersurface.h",
            "cpp/style.cpp",
            "cpp/style.h",
            "cpp/shim.cpp",
            "cpp/shim.h",
        ])
        .include_dir("cpp")
        .include_dir("/usr/include/LayerShellQt")
        .qt_module("Quick")
        .qt_module("Gui")
        .qt_module("Core")
        .build();

    println!("cargo:rustc-link-lib=LayerShellQtInterface");
}
