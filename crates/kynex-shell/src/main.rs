//! Kynex Shell main entry point.
//!
//! Hosts the Wayland layer-shell presentation layer driven by swappable style packs.

pub mod clock;
pub mod network;
pub mod power;
pub mod shim;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use std::path::{Path, PathBuf};
use tracing::{error, info};

fn find_style_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KYNEX_STYLE_DIR") {
        return PathBuf::from(dir);
    }

    let local_style = Path::new("styles/default");
    if local_style.exists() {
        return local_style.to_path_buf();
    }

    let system_style = Path::new("/usr/share/kynex/styles/default");
    if system_style.exists() {
        return system_style.to_path_buf();
    }

    PathBuf::from("styles/default")
}

fn find_qml_import_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KYNEX_QML_IMPORT_DIR") {
        return PathBuf::from(dir);
    }

    let local_qml = Path::new("crates/kynex-shell/qml");
    if local_qml.exists() {
        return local_qml.to_path_buf();
    }

    let system_qml = Path::new("/usr/lib/kynex/qml");
    if system_qml.exists() {
        return system_qml.to_path_buf();
    }

    PathBuf::from("crates/kynex-shell/qml")
}

fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting Kynex Shell (Phase 1)");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .ok();
    let _guard = rt.as_ref().map(tokio::runtime::Runtime::enter);

    let style_dir = find_style_dir();
    let (manifest, tokens) = match kynex_style::load_style_pack(&style_dir) {
        Ok(pack) => pack,
        Err(e) => {
            error!("Failed to load style pack from {:?}: {e}", style_dir);
            std::process::exit(1);
        }
    };

    let canonical_style_dir = match style_dir.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to resolve style directory {:?}: {e}", style_dir);
            std::process::exit(1);
        }
    };
    let shell_qml_path = canonical_style_dir.join(&manifest.entry.shell);

    let mut app = QGuiApplication::new();

    // Register C++ LayerSurface and Style singleton into Kynex.Shell and Kynex.Style
    shim::register_types();

    // Feed parsed design tokens into the Style singleton
    info!(
        "Applying design tokens (unit: {}, typography: {}px)",
        tokens.spacing.unit, tokens.typography.base_px
    );
    shim::init_tokens(&tokens);

    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        let style_components_dir = canonical_style_dir.join("components");
        if style_components_dir.is_dir() {
            let comp_str = style_components_dir.to_string_lossy();
            info!("Adding style components override path: {comp_str}");
            engine
                .as_mut()
                .add_import_path(&QString::from(comp_str.as_ref()));
        }

        let qml_import_dir = find_qml_import_dir();
        if let Ok(canonical_qml) = qml_import_dir.canonicalize() {
            let import_str = canonical_qml.to_string_lossy();
            info!("Adding QML import path: {import_str}");
            engine
                .as_mut()
                .add_import_path(&QString::from(import_str.as_ref()));
        }

        let qurl_str = format!("file://{}", shell_qml_path.to_string_lossy());
        info!("Loading style shell from {qurl_str}");
        engine.as_mut().load(&QUrl::from(&qurl_str));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
