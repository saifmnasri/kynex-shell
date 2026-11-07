//! Unit and integration tests for icon lookup.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::similar_names,
    clippy::manual_let_else,
    clippy::print_stdout
)]

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use tempfile::TempDir;

use kynex_icons::lookup::{normalize_icon_name, validate_icon_name, IconResolver};
use kynex_icons::roots::IconRoots;
use kynex_icons::theme::{DirectoryType, SubdirectoryMeta};

fn create_theme_fixture(dir: &Path, theme_name: &str, content: &str) {
    let theme_dir = dir.join(theme_name);
    fs::create_dir_all(&theme_dir).unwrap();
    let mut file = File::create(theme_dir.join("index.theme")).unwrap();
    file.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_icon_name_validation_and_normalization() {
    assert_eq!(validate_icon_name("firefox").unwrap(), "firefox");
    assert_eq!(
        validate_icon_name("org.kynexos.Shell").unwrap(),
        "org.kynexos.Shell"
    );
    assert_eq!(validate_icon_name("app-name_2.0").unwrap(), "app-name_2.0");

    assert_eq!(normalize_icon_name("firefox.png"), "firefox");
    assert_eq!(normalize_icon_name("firefox.svg"), "firefox");
    assert_eq!(normalize_icon_name("firefox"), "firefox");

    // Hostile inputs
    assert!(validate_icon_name("").is_err());
    assert!(validate_icon_name("../../etc/passwd").is_err());
    assert!(validate_icon_name("foo/bar").is_err());
    assert!(validate_icon_name("foo\\bar").is_err());
    assert!(validate_icon_name("foo\0bar").is_err());
    assert!(validate_icon_name("foo\nbar").is_err());
    assert!(validate_icon_name(&"a".repeat(129)).is_err());
    assert!(validate_icon_name("semi;colon").is_err());
    assert!(validate_icon_name("pipe|bad").is_err());
}

#[test]
fn test_subdirectory_matching_and_distance() {
    // Fixed: nominal 32
    let fixed = SubdirectoryMeta {
        name: "32x32/apps".into(),
        size: 32,
        scale: 1,
        dir_type: DirectoryType::Fixed,
        min_size: 32,
        max_size: 32,
        threshold: 2,
    };
    assert!(fixed.matches_size(32, 1));
    assert!(!fixed.matches_size(32, 2));
    assert!(!fixed.matches_size(48, 1));
    assert_eq!(fixed.size_distance(32, 1), 0);
    assert_eq!(fixed.size_distance(48, 1), 16);

    // Scalable: 16 to 128
    let scalable = SubdirectoryMeta {
        name: "scalable/apps".into(),
        size: 48,
        scale: 1,
        dir_type: DirectoryType::Scalable,
        min_size: 16,
        max_size: 128,
        threshold: 2,
    };
    assert!(scalable.matches_size(16, 1));
    assert!(scalable.matches_size(48, 1));
    assert!(scalable.matches_size(128, 1));
    assert!(!scalable.matches_size(256, 1));
    assert_eq!(scalable.size_distance(48, 1), 0);
    assert_eq!(scalable.size_distance(140, 1), 12);
    assert_eq!(scalable.size_distance(10, 1), 6);

    // Threshold: 48 +- 2 -> [46, 50]
    let threshold = SubdirectoryMeta {
        name: "48x48/apps".into(),
        size: 48,
        scale: 1,
        dir_type: DirectoryType::Threshold,
        min_size: 48,
        max_size: 48,
        threshold: 2,
    };
    assert!(threshold.matches_size(46, 1));
    assert!(threshold.matches_size(48, 1));
    assert!(threshold.matches_size(50, 1));
    assert!(!threshold.matches_size(51, 1));
    assert!(!threshold.matches_size(45, 1));
    assert_eq!(threshold.size_distance(48, 1), 0);
    assert_eq!(threshold.size_distance(54, 1), 4);
}

#[test]
fn test_theme_hierarchy_inheritance_and_cycle_detection() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("icons");
    fs::create_dir_all(&root).unwrap();

    // Cyclic inheritance: ThemeA -> ThemeB -> ThemeA
    create_theme_fixture(
        &root,
        "ThemeA",
        "[Icon Theme]\nName=ThemeA\nInherits=ThemeB\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "ThemeB",
        "[Icon Theme]\nName=ThemeB\nInherits=ThemeA,hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "hicolor",
        "[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );

    // Add icon only in hicolor
    let hicolor_apps = root.join("hicolor/48x48/apps");
    fs::create_dir_all(&hicolor_apps).unwrap();
    File::create(hicolor_apps.join("app.png"))
        .unwrap()
        .write_all(b"fake png")
        .unwrap();

    let roots = IconRoots::custom(vec![root.clone()], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    // Should resolve app in hicolor without infinite loop
    let found = resolver.find_icon("app", 48, 1, "ThemeA").unwrap();
    assert!(found.is_some());
    assert!(found.unwrap().ends_with("hicolor/48x48/apps/app.png"));
}

#[test]
fn test_pixmap_fallback() {
    let tmp = TempDir::new().unwrap();
    let icons_root = tmp.path().join("icons");
    let pixmaps_root = tmp.path().join("pixmaps");
    fs::create_dir_all(&icons_root).unwrap();
    fs::create_dir_all(&pixmaps_root).unwrap();

    create_theme_fixture(
        &icons_root,
        "hicolor",
        "[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );

    // Put icon in pixmaps
    let pixmap_file = pixmaps_root.join("legacy_app.svg");
    File::create(&pixmap_file)
        .unwrap()
        .write_all(b"<svg></svg>")
        .unwrap();

    let roots = IconRoots::custom(vec![icons_root], vec![pixmaps_root.clone()], vec![]);
    let resolver = IconResolver::new(roots);

    let found = resolver.find_icon("legacy_app", 48, 1, "hicolor").unwrap();
    assert_eq!(found, Some(fs::canonicalize(pixmap_file).unwrap()));
}

#[test]
fn test_hostile_file_limits_and_symlinks() {
    let tmp = TempDir::new().unwrap();
    let icons_root = tmp.path().join("icons");
    fs::create_dir_all(&icons_root).unwrap();

    create_theme_fixture(
        &icons_root,
        "hicolor",
        "[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    let apps_dir = icons_root.join("hicolor/48x48/apps");
    fs::create_dir_all(&apps_dir).unwrap();

    // 1. Symlink pointing outside root to /etc/passwd
    let evil_symlink = apps_dir.join("evil_passwd.png");
    symlink("/etc/passwd", &evil_symlink).unwrap();

    // 2. Symlink loop: loop1 -> loop2 -> loop1
    let loop1 = apps_dir.join("loop1.png");
    let loop2 = apps_dir.join("loop2.png");
    symlink(&loop2, &loop1).unwrap();
    symlink(&loop1, &loop2).unwrap();

    // 3. Oversized file (> 1 MiB)
    let huge_file = apps_dir.join("huge.png");
    let f = File::create(&huge_file).unwrap();
    f.set_len(2 * 1024 * 1024).unwrap(); // 2 MiB

    let roots = IconRoots::custom(vec![icons_root.clone()], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    // Rejections:
    assert_eq!(
        resolver.find_icon("evil_passwd", 48, 1, "hicolor").unwrap(),
        None
    );
    assert_eq!(resolver.find_icon("loop1", 48, 1, "hicolor").unwrap(), None);
    assert_eq!(resolver.find_icon("huge", 48, 1, "hicolor").unwrap(), None);
}

#[test]
fn test_absolute_path_icon_rules() {
    let tmp = TempDir::new().unwrap();
    let icons_root = tmp.path().join("icons");
    let xdg_data_root = tmp.path().join("share");
    let outside_dir = tmp.path().join("outside");
    fs::create_dir_all(&icons_root).unwrap();
    fs::create_dir_all(&xdg_data_root).unwrap();
    fs::create_dir_all(&outside_dir).unwrap();

    let valid_icon = xdg_data_root.join("app.png");
    File::create(&valid_icon)
        .unwrap()
        .write_all(b"png")
        .unwrap();

    let outside_icon = outside_dir.join("app.png");
    File::create(&outside_icon)
        .unwrap()
        .write_all(b"png")
        .unwrap();

    let roots = IconRoots::custom(vec![icons_root], vec![], vec![xdg_data_root]);
    let resolver = IconResolver::new(roots);

    // Inside XDG data root: accepted
    let found = resolver
        .find_icon(valid_icon.to_str().unwrap(), 48, 1, "hicolor")
        .unwrap();
    assert_eq!(found, Some(fs::canonicalize(valid_icon).unwrap()));

    // Outside allowed roots: rejected
    assert!(resolver
        .find_icon(outside_icon.to_str().unwrap(), 48, 1, "hicolor")
        .is_err());
    // Absolute path to /etc/passwd: rejected
    assert!(resolver.find_icon("/etc/passwd", 48, 1, "hicolor").is_err());
}

#[test]
fn test_config_and_roots_from_env() {
    let roots = IconRoots::from_env();
    assert!(!roots.theme_roots.is_empty());
    assert!(!roots.canonical_allowed_roots.is_empty());

    // Test resolve_active_theme default
    let active_theme = kynex_icons::config::resolve_active_theme(&roots);
    assert!(!active_theme.is_empty());

    // Test config reader with custom config.toml
    let tmp = TempDir::new().unwrap();
    let cfg_path = tmp.path().join("config.toml");
    fs::write(&cfg_path, "[icons]\ntheme = \"my-theme\"\n").unwrap();
    let theme_read = kynex_icons::config::read_theme_from_path(&cfg_path);
    assert_eq!(theme_read, Some("my-theme".to_string()));

    // Corrupt / empty config
    let empty_cfg = tmp.path().join("empty.toml");
    fs::write(&empty_cfg, "").unwrap();
    assert_eq!(kynex_icons::config::read_theme_from_path(&empty_cfg), None);

    let bad_cfg = tmp.path().join("bad.toml");
    fs::write(&bad_cfg, "bad = [").unwrap();
    assert_eq!(kynex_icons::config::read_theme_from_path(&bad_cfg), None);

    // Test convenience lookup_icon function
    let res = kynex_icons::lookup_icon("nonexistent-app-12345", 48, 1, None);
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), None);

    // Cache clear
    let cache = kynex_icons::IconCache::new(32);
    cache.clear();
}
