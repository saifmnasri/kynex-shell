//! Kynex Shell main entry point.
//!
//! Hosts the Wayland layer-shell presentation layer driven by swappable style packs.

pub mod clock;
pub mod shim;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
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

fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting Kynex Shell (Phase 1)");

    let style_dir = find_style_dir();
    let (manifest, _tokens) = match kynex_style::load_style_pack(&style_dir) {
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

    // Register C++ LayerSurface into org.kynexos.core
    shim::register_types();

    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        let qurl_str = format!("file://{}", shell_qml_path.to_string_lossy());
        info!("Loading style shell from {qurl_str}");
        engine.load(&QUrl::from(&qurl_str));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
