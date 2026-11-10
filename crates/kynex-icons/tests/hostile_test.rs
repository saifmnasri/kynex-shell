//! Hostile corpus tests for icon files, theme indices, and decoding boundaries.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::similar_names,
    clippy::print_stdout
)]

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tempfile::TempDir;

use kynex_icons::lookup::{validate_icon_name, IconResolver};
use kynex_icons::roots::IconRoots;

fn create_theme_structure(root: &Path, theme_name: &str) -> PathBuf {
    let theme_dir = root.join(theme_name);
    fs::create_dir_all(&theme_dir).unwrap();
    let index_path = theme_dir.join("index.theme");
    fs::write(
        &index_path,
        "[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    )
    .unwrap();

    let apps_dir = theme_dir.join("48x48/apps");
    fs::create_dir_all(&apps_dir).unwrap();
    apps_dir
}

#[test]
fn test_hostile_names_and_path_traversal() {
    assert!(validate_icon_name("../../x").is_err());
    assert!(validate_icon_name("../etc/passwd").is_err());
    assert!(validate_icon_name("app/sub").is_err());
    assert!(validate_icon_name("app\\sub").is_err());
    assert!(validate_icon_name("app\0null").is_err());
    assert!(validate_icon_name("").is_err());
    assert!(validate_icon_name(&"x".repeat(129)).is_err());
}

#[test]
fn test_hostile_100mb_file_rejected() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("icons");
    let apps = create_theme_structure(&root, "hicolor");

    let big_path = apps.join("huge_app.png");
    let f = File::create(&big_path).unwrap();
    f.set_len(100 * 1024 * 1024).unwrap(); // 100 MB sparse file

    let roots = IconRoots::custom(vec![root], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    let res = resolver.find_icon("huge_app", 48, 1, "hicolor").unwrap();
    assert_eq!(
        res, None,
        "100 MB file must be rejected exceeding 1 MiB limit"
    );
}

#[test]
fn test_hostile_fifo_rejected() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("icons");
    let apps = create_theme_structure(&root, "hicolor");

    let fifo_path = apps.join("fifo_app.png");
    let status = Command::new("mkfifo")
        .arg(&fifo_path)
        .status()
        .expect("mkfifo command failed");
    assert!(status.success());

    let roots = IconRoots::custom(vec![root], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    let res = resolver.find_icon("fifo_app", 48, 1, "hicolor").unwrap();
    assert_eq!(
        res, None,
        "FIFO special file must be rejected as non-regular file"
    );
}

#[test]
fn test_hostile_symlink_to_passwd_rejected() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("icons");
    let apps = create_theme_structure(&root, "hicolor");

    let evil_link = apps.join("evil_app.png");
    symlink("/etc/passwd", &evil_link).unwrap();

    let roots = IconRoots::custom(vec![root], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    let res = resolver.find_icon("evil_app", 48, 1, "hicolor").unwrap();
    assert_eq!(
        res, None,
        "Symlink escaping root to /etc/passwd must be rejected"
    );
}

#[test]
fn test_hostile_symlink_loop_rejected() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("icons");
    let apps = create_theme_structure(&root, "hicolor");

    let link_a = apps.join("loop_a.png");
    let link_b = apps.join("loop_b.png");
    symlink(&link_b, &link_a).unwrap();
    symlink(&link_a, &link_b).unwrap();

    let roots = IconRoots::custom(vec![root], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    let res = resolver.find_icon("loop_a", 48, 1, "hicolor").unwrap();
    assert_eq!(
        res, None,
        "Cyclic symlink loop must resolve safely to None without panic"
    );
}

#[test]
fn test_hostile_absolute_path_outside_roots() {
    let tmp = TempDir::new().unwrap();
    let icons_root = tmp.path().join("icons");
    let forbidden_dir = tmp.path().join("forbidden");
    fs::create_dir_all(&forbidden_dir).unwrap();

    let secret_file = forbidden_dir.join("secret.png");
    File::create(&secret_file)
        .unwrap()
        .write_all(b"secret")
        .unwrap();

    let roots = IconRoots::custom(vec![icons_root], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    assert!(resolver
        .find_icon(secret_file.to_str().unwrap(), 48, 1, "hicolor")
        .is_err());
}

#[test]
fn test_10000_icon_theme_performance() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("icons");
    let apps = create_theme_structure(&root, "hicolor");

    // Generate 10,000 dummy icon files
    for i in 0..10_000 {
        let path = apps.join(format!("icon_{i}.png"));
        // Create 0-byte dummy file
        File::create(&path).unwrap();
    }

    let roots = IconRoots::custom(vec![root], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    // Measure cold lookup
    let t0 = Instant::now();
    let first = resolver.find_icon("icon_0", 48, 1, "hicolor").unwrap();
    let cold_dur = t0.elapsed();
    assert!(first.is_some());

    // Measure warm (cached) lookup
    let t1 = Instant::now();
    let warm = resolver.find_icon("icon_0", 48, 1, "hicolor").unwrap();
    let warm_dur = t1.elapsed();
    assert_eq!(first, warm);

    // Measure non-existent lookup in 10,000 entries
    let t2 = Instant::now();
    let missing = resolver
        .find_icon("icon_99999_missing", 48, 1, "hicolor")
        .unwrap();
    let missing_dur = t2.elapsed();
    assert_eq!(missing, None);

    // In-memory directory index should make lookup very fast
    assert!(cold_dur.as_millis() < 250, "Cold lookup took {cold_dur:?}");
    assert!(warm_dur.as_micros() < 200, "Warm lookup took {warm_dur:?}");
    assert!(
        missing_dur.as_micros() < 500,
        "Missing lookup took {missing_dur:?}"
    );
}
