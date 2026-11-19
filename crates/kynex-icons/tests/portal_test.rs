#![allow(
    clippy::print_stdout,
    clippy::uninlined_format_args,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::len_zero
)]
//! Integration test for reading the system icon theme from xdg-desktop-portal Settings
//! and verifying icon resolution for real system applications.

use kynex_icons::config::{fetch_portal_icon_theme_async, resolve_active_theme};
use kynex_icons::lookup::IconResolver;
use kynex_icons::roots::IconRoots;

#[tokio::test]
async fn test_query_portal_settings() {
    let theme = fetch_portal_icon_theme_async().await;
    println!("Portal icon theme: {theme:?}");
    if let Some(t) = theme {
        assert!(!t.is_empty(), "Portal returned empty theme string");
    }
}

#[test]
fn test_resolve_ten_system_apps_icons() {
    let roots = IconRoots::from_env();
    let theme = resolve_active_theme(&roots);
    println!("Active icon theme resolved: {theme}");

    let apps_to_test = vec![
        ("Calculator", "org.gnome.Calculator"),
        ("Files (Nautilus)", "org.gnome.Nautilus"),
        ("Terminal (Ptyxis)", "org.gnome.Ptyxis"),
        ("Settings", "org.gnome.Settings"),
        ("Text Editor", "org.gnome.TextEditor"),
        ("Software Updater", "system-software-update"),
        ("Bluetooth", "bluetooth"),
        ("Power Panel", "org.gnome.Settings-power-symbolic"),
        ("Network Panel", "org.gnome.Settings-network-symbolic"),
        (
            "Apport Crash Reporter (Fallback)",
            "apport-nonexistent-fallback",
        ),
    ];

    let resolver = IconResolver::new(roots);
    let mut resolved_count = 0;
    let mut fallback_count = 0;

    println!("=== 10 Application Icon Lookups ===");
    for (app_name, icon_name) in apps_to_test {
        match resolver.find_icon(icon_name, 32, 1, &theme) {
            Ok(Some(path)) => {
                println!("[RESOLVED] {app_name} ('{icon_name}') -> {path:?}");
                resolved_count += 1;
            }
            Ok(None) => {
                println!(
                    "[FALLBACK BADGE] {app_name} ('{icon_name}') -> None (fallback letter badge)"
                );
                fallback_count += 1;
            }
            Err(e) => {
                panic!("Lookup failed with unexpected error for {icon_name}: {e}");
            }
        }
    }

    println!("Total resolved: {resolved_count}, Fallback badge: {fallback_count}");
    assert!(
        resolved_count >= 8,
        "Expected at least 8 apps to resolve icons in Yaru/hicolor, got {resolved_count}"
    );
    assert_eq!(
        fallback_count, 1,
        "Expected exactly 1 fallback badge for apport-nonexistent-fallback"
    );
}
