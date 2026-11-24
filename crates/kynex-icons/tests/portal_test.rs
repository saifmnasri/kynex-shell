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
use tempfile::TempDir;

fn setup_fallback_icons_fixture() -> TempDir {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let hicolor_apps = tmp.path().join("icons/hicolor/32x32/apps");
    std::fs::create_dir_all(&hicolor_apps).expect("create hicolor apps dir");

    let theme_content = "\
[Icon Theme]
Name=Hicolor
Comment=Fallback icon theme
Directories=32x32/apps

[32x32/apps]
Context=Applications
Size=32
Type=Fixed
";
    std::fs::write(tmp.path().join("icons/hicolor/index.theme"), theme_content)
        .expect("write index.theme");

    // 1x1 valid transparent PNG
    let png_1x1: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0x63, 0x34, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    let icon_names = [
        "org.gnome.Calculator",
        "org.gnome.Nautilus",
        "org.gnome.Ptyxis",
        "org.gnome.Settings",
        "org.gnome.TextEditor",
        "system-software-update",
        "bluetooth",
        "org.gnome.Settings-power-symbolic",
        "org.gnome.Settings-network-symbolic",
    ];

    for name in icon_names {
        std::fs::write(hicolor_apps.join(format!("{name}.png")), png_1x1)
            .expect("write icon fixture");
    }

    tmp
}

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
    let fixture = setup_fallback_icons_fixture();
    let old_data_dirs = std::env::var_os("XDG_DATA_DIRS");
    let mut new_dirs = vec![fixture.path().to_path_buf()];
    if let Some(ref old) = old_data_dirs {
        new_dirs.extend(std::env::split_paths(old));
    } else {
        new_dirs.push(std::path::PathBuf::from("/usr/local/share"));
        new_dirs.push(std::path::PathBuf::from("/usr/share"));
    }
    let joined = std::env::join_paths(&new_dirs).expect("join paths");
    std::env::set_var("XDG_DATA_DIRS", &joined);

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

    if let Some(old) = old_data_dirs {
        std::env::set_var("XDG_DATA_DIRS", old);
    } else {
        std::env::remove_var("XDG_DATA_DIRS");
    }

    assert!(
        resolved_count >= 8,
        "Expected at least 8 apps to resolve icons in Yaru/hicolor, got {resolved_count}"
    );
    assert_eq!(
        fallback_count, 1,
        "Expected exactly 1 fallback badge for apport-nonexistent-fallback"
    );
}
