//! Unit tests for window to application matching logic.
//!
//! Tests realistic window class names installed on the host machine and hostile inputs.

#![allow(clippy::unwrap_used)]

use kynex_shell_core::matching::{
    extract_reverse_dns_leaf, match_window_to_app, normalize_for_comparison, strip_desktop_suffix,
    AppCandidate,
};

fn sample_candidates() -> Vec<AppCandidate> {
    vec![
        // Realistic Ubuntu 26.04 desktop entries
        AppCandidate::new(
            "org.gnome.Terminal.desktop",
            Some("gnome-terminal-server"),
            "org.gnome.Terminal",
        ),
        AppCandidate::new(
            "gnome-initial-setup.desktop",
            Some("org.gnome.InitialSetup"),
            "preferences-system",
        ),
        AppCandidate::new(
            "org.gnome.Calculator.desktop",
            None::<String>,
            "org.gnome.Calculator",
        ),
        AppCandidate::new(
            "org.gnome.Nautilus.desktop",
            None::<String>,
            "org.gnome.Nautilus",
        ),
        AppCandidate::new(
            "org.gnome.Ptyxis.desktop",
            None::<String>,
            "org.gnome.Ptyxis",
        ),
        AppCandidate::new(
            "org.gnome.TextEditor.desktop",
            None::<String>,
            "org.gnome.TextEditor",
        ),
        AppCandidate::new(
            "org.gnome.DiskUtility.desktop",
            None::<String>,
            "org.gnome.DiskUtility",
        ),
        AppCandidate::new(
            "net.nokyan.Resources.desktop",
            None::<String>,
            "net.nokyan.Resources",
        ),
        AppCandidate::new(
            "nm-connection-editor.desktop",
            None::<String>,
            "preferences-system-network",
        ),
        AppCandidate::new(
            "update-manager.desktop",
            None::<String>,
            "system-software-update",
        ),
        AppCandidate::new("firefox.desktop", Some("firefox"), "firefox"),
    ]
}

#[test]
fn test_suffix_stripping_and_helpers() {
    assert_eq!(strip_desktop_suffix("firefox.desktop"), "firefox");
    assert_eq!(strip_desktop_suffix("firefox"), "firefox");
    assert_eq!(
        strip_desktop_suffix("org.gnome.Nautilus.desktop"),
        "org.gnome.Nautilus"
    );

    assert_eq!(
        extract_reverse_dns_leaf("org.gnome.Nautilus.desktop"),
        "Nautilus"
    );
    assert_eq!(extract_reverse_dns_leaf("firefox"), "firefox");
    assert_eq!(extract_reverse_dns_leaf("com.example.sub.App"), "App");

    assert_eq!(
        normalize_for_comparison("gnome-text-editor.desktop"),
        "gnometexteditor"
    );
    assert_eq!(normalize_for_comparison("TextEditor"), "texteditor");
    assert_eq!(
        normalize_for_comparison("nm_connection_editor"),
        "nmconnectioneditor"
    );
}

#[test]
fn test_realistic_installed_apps_matching() {
    let candidates = sample_candidates();

    // 1. StartupWMClass match: gnome-terminal-server -> org.gnome.Terminal.desktop
    let match_term = match_window_to_app("gnome-terminal-server", &candidates);
    assert!(match_term.is_some());
    assert_eq!(match_term.unwrap().id, "org.gnome.Terminal.desktop");

    // 2. StartupWMClass match: org.gnome.InitialSetup -> gnome-initial-setup.desktop
    let match_setup = match_window_to_app("org.gnome.InitialSetup", &candidates);
    assert!(match_setup.is_some());
    assert_eq!(match_setup.unwrap().id, "gnome-initial-setup.desktop");

    // 3. Exact full appId match: org.gnome.Calculator -> org.gnome.Calculator.desktop
    let match_calc = match_window_to_app("org.gnome.Calculator", &candidates);
    assert!(match_calc.is_some());
    assert_eq!(match_calc.unwrap().id, "org.gnome.Calculator.desktop");

    // 4. Reverse-DNS leaf match: nautilus -> org.gnome.Nautilus.desktop
    let match_nautilus = match_window_to_app("nautilus", &candidates);
    assert!(match_nautilus.is_some());
    assert_eq!(match_nautilus.unwrap().id, "org.gnome.Nautilus.desktop");

    // 5. Case-folded leaf match: Nautilus -> org.gnome.Nautilus.desktop
    let match_nautilus_cap = match_window_to_app("Nautilus", &candidates);
    assert!(match_nautilus_cap.is_some());
    assert_eq!(match_nautilus_cap.unwrap().id, "org.gnome.Nautilus.desktop");

    // 6. Reverse-DNS leaf match: ptyxis -> org.gnome.Ptyxis.desktop
    let match_ptyxis = match_window_to_app("ptyxis", &candidates);
    assert!(match_ptyxis.is_some());
    assert_eq!(match_ptyxis.unwrap().id, "org.gnome.Ptyxis.desktop");

    // 7. Hyphenated normalized match: gnome-text-editor -> org.gnome.TextEditor.desktop
    let match_editor = match_window_to_app("gnome-text-editor", &candidates);
    assert!(match_editor.is_some());
    assert_eq!(match_editor.unwrap().id, "org.gnome.TextEditor.desktop");

    // 8. Reverse-DNS leaf match: resources -> net.nokyan.Resources.desktop
    let match_res = match_window_to_app("resources", &candidates);
    assert!(match_res.is_some());
    assert_eq!(match_res.unwrap().id, "net.nokyan.Resources.desktop");

    // 9. Exact simple ID match: nm-connection-editor -> nm-connection-editor.desktop
    let match_nm = match_window_to_app("nm-connection-editor", &candidates);
    assert!(match_nm.is_some());
    assert_eq!(match_nm.unwrap().id, "nm-connection-editor.desktop");

    // 10. Trailing .desktop on window app_id handled cleanly
    let match_dot_desktop = match_window_to_app("update-manager.desktop", &candidates);
    assert!(match_dot_desktop.is_some());
    assert_eq!(match_dot_desktop.unwrap().id, "update-manager.desktop");
}

#[test]
fn test_hostile_matching_inputs() {
    let candidates = sample_candidates();

    // Empty and whitespace
    assert_eq!(match_window_to_app("", &candidates), None);
    assert_eq!(match_window_to_app("   ", &candidates), None);
    assert_eq!(match_window_to_app("\t\n", &candidates), None);

    // Completely unknown ID
    assert_eq!(
        match_window_to_app("com.unknown.nonexistent", &candidates),
        None
    );
    assert_eq!(match_window_to_app("evil-binary-404", &candidates), None);

    // Path traversal strings
    assert_eq!(match_window_to_app("../../etc/passwd", &candidates), None);
    assert_eq!(match_window_to_app("/usr/bin/sudo", &candidates), None);

    // NUL bytes and control characters
    assert_eq!(
        match_window_to_app("org.gnome.Calculator\0evil", &candidates),
        None
    );
    assert_eq!(match_window_to_app("nautilus\x1b[31m", &candidates), None);

    // 10,000-character adversarial string
    let huge_input = "A".repeat(10_000);
    assert_eq!(match_window_to_app(&huge_input, &candidates), None);
}
