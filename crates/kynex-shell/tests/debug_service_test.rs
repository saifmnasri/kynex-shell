//! Unit tests for `org.kynexos.Shell.Debug1` scene introspection and screenshot capture (Stage R4.2).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::case_sensitive_file_extension_comparisons
)]

use kynex_shell::shim::{dump_active_scene, grab_active_panel};
use serde_json::Value;

#[test]
fn test_dump_active_scene_sanitization_and_structure() {
    let json_str = dump_active_scene();
    assert!(
        !json_str.is_empty(),
        "dump_active_scene must not return empty string"
    );

    let parsed: Value =
        serde_json::from_str(&json_str).expect("dump_active_scene must return valid JSON");

    // When no panel window is loaded, returns {"error":"no window"}
    // When loaded, contains sanitized keys
    if let Some(err) = parsed.get("error") {
        assert_eq!(err.as_str(), Some("no window"));
    } else {
        assert!(parsed.get("panelGeometry").is_some());
        assert!(parsed.get("windowSize").is_some());
        assert!(parsed.get("devicePixelRatio").is_some());
        assert!(parsed.get("screenGeometry").is_some());
        assert!(parsed.get("delegateCount").is_some());
        assert!(parsed.get("modelRowCount").is_some());

        // Hard security invariant: Strictly NO titles in the dump
        assert!(parsed.get("title").is_none());
        assert!(parsed.get("windowTitle").is_none());
    }
}

#[test]
fn test_grab_active_panel_mode_and_path() {
    let path = grab_active_panel();
    if !path.is_empty() {
        assert!(path.contains("panel-"));
        assert!(path.ends_with(".png"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&path).expect("Grabbed panel file must exist");
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Panel screenshot must have strict mode 0600");
        }
    }
}
