//! Integration test verifying QML calls to real `TrayMenuModel` (M1.4).

#![allow(clippy::unwrap_used)]

use kynex_shell::shim::test_real_tray_menu_model_open;

#[tokio::test]
async fn test_real_tray_menu_model_qml_call() {
    let qml_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/qml_tests/real_tray_menu_model_test.qml"
    );
    assert!(
        test_real_tray_menu_model_open(qml_path),
        "Real TrayMenuModel must receive open() call with 15 production arguments from QML"
    );
}
