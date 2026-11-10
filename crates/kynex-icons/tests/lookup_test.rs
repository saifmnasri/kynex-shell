//! Unit and integration tests for icon lookup.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::similar_names,
    clippy::manual_let_else,
    clippy::print_stdout,
    clippy::too_many_lines
)]

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
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

#[test]
fn test_inheritance_order_spec_compliance() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("icons");
    fs::create_dir_all(&root).unwrap();

    // 1. Inherits=ThemeA,ThemeB with same icon in both: ThemeA must win!
    create_theme_fixture(
        &root,
        "ChildAB",
        "[Icon Theme]\nName=ChildAB\nInherits=ThemeA,ThemeB\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "ThemeA",
        "[Icon Theme]\nName=ThemeA\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "ThemeB",
        "[Icon Theme]\nName=ThemeB\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    let dir_a = root.join("ThemeA/48x48/apps");
    fs::create_dir_all(&dir_a).unwrap();
    File::create(dir_a.join("same_icon.png"))
        .unwrap()
        .write_all(b"from A")
        .unwrap();

    let dir_b = root.join("ThemeB/48x48/apps");
    fs::create_dir_all(&dir_b).unwrap();
    File::create(dir_b.join("same_icon.png"))
        .unwrap()
        .write_all(b"from B")
        .unwrap();

    let roots = IconRoots::custom(vec![root.clone()], vec![], vec![]);
    let resolver = IconResolver::new(roots);

    let res_ab = resolver
        .find_icon("same_icon", 48, 1, "ChildAB")
        .unwrap()
        .unwrap();
    assert!(
        res_ab.ends_with("ThemeA/48x48/apps/same_icon.png"),
        "ThemeA must win over ThemeB when listed first in Inherits. Resolved: {res_ab:?}"
    );

    // 2. A inherits A2 and B has the icon too: A2 before B (depth-first traversal!)
    create_theme_fixture(
        &root,
        "ChildTree",
        "[Icon Theme]\nName=ChildTree\nInherits=ParentA,ParentB\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "ParentA",
        "[Icon Theme]\nName=ParentA\nInherits=ParentA2\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "ParentA2",
        "[Icon Theme]\nName=ParentA2\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "ParentB",
        "[Icon Theme]\nName=ParentB\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    let dir_a2 = root.join("ParentA2/48x48/apps");
    fs::create_dir_all(&dir_a2).unwrap();
    File::create(dir_a2.join("tree_icon.png"))
        .unwrap()
        .write_all(b"from A2")
        .unwrap();

    let dir_parent_b = root.join("ParentB/48x48/apps");
    fs::create_dir_all(&dir_parent_b).unwrap();
    File::create(dir_parent_b.join("tree_icon.png"))
        .unwrap()
        .write_all(b"from B")
        .unwrap();

    let res_tree = resolver
        .find_icon("tree_icon", 48, 1, "ChildTree")
        .unwrap()
        .unwrap();
    assert!(
        res_tree.ends_with("ParentA2/48x48/apps/tree_icon.png"),
        "ParentA's ancestor ParentA2 must be searched before second parent ParentB. Resolved: {res_tree:?}"
    );

    // 3. Diamond inheritance: Child -> (A, B) -> Base
    create_theme_fixture(
        &root,
        "DiamondChild",
        "[Icon Theme]\nName=DiamondChild\nInherits=DiamondA,DiamondB\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "DiamondA",
        "[Icon Theme]\nName=DiamondA\nInherits=DiamondBase\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "DiamondB",
        "[Icon Theme]\nName=DiamondB\nInherits=DiamondBase\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "DiamondBase",
        "[Icon Theme]\nName=DiamondBase\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    let dir_base = root.join("DiamondBase/48x48/apps");
    fs::create_dir_all(&dir_base).unwrap();
    File::create(dir_base.join("diamond_icon.png"))
        .unwrap()
        .write_all(b"from DiamondBase")
        .unwrap();

    let res_diamond = resolver
        .find_icon("diamond_icon", 48, 1, "DiamondChild")
        .unwrap()
        .unwrap();
    assert!(
        res_diamond.ends_with("DiamondBase/48x48/apps/diamond_icon.png"),
        "Diamond inheritance must resolve icon in DiamondBase cleanly. Resolved: {res_diamond:?}"
    );

    // 4. Cycle detection
    create_theme_fixture(
        &root,
        "CycleA",
        "[Icon Theme]\nName=CycleA\nInherits=CycleB\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    create_theme_fixture(
        &root,
        "CycleB",
        "[Icon Theme]\nName=CycleB\nInherits=CycleA\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    // Neither has cycle_missing_icon; hicolor has it
    create_theme_fixture(
        &root,
        "hicolor",
        "[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    let dir_hicolor = root.join("hicolor/48x48/apps");
    fs::create_dir_all(&dir_hicolor).unwrap();
    File::create(dir_hicolor.join("cycle_missing_icon.png"))
        .unwrap()
        .write_all(b"from hicolor")
        .unwrap();

    let res_cycle = resolver
        .find_icon("cycle_missing_icon", 48, 1, "CycleA")
        .unwrap()
        .unwrap();
    assert!(
        res_cycle.ends_with("hicolor/48x48/apps/cycle_missing_icon.png"),
        "Cyclic themes must terminate cleanly and fall through to hicolor. Resolved: {res_cycle:?}"
    );

    // 5. Depth cap (8)
    // Build chain: T0 -> T1 -> ... -> T8 -> T9
    for i in 0..=9 {
        let inherits = if i < 9 {
            format!("Inherits=T{}\n", i + 1)
        } else {
            String::new()
        };
        create_theme_fixture(
            &root,
            &format!("T{i}"),
            &format!("[Icon Theme]\nName=T{i}\n{inherits}Directories=48x48/apps\n\n[48x48/apps]\nSize=48\n"),
        );
    }
    // Icon placed at T8 (depth 8)
    let dir_t8 = root.join("T8/48x48/apps");
    fs::create_dir_all(&dir_t8).unwrap();
    File::create(dir_t8.join("depth8_icon.png"))
        .unwrap()
        .write_all(b"depth 8")
        .unwrap();

    // Icon placed at T9 (depth 9)
    let dir_t9 = root.join("T9/48x48/apps");
    fs::create_dir_all(&dir_t9).unwrap();
    File::create(dir_t9.join("depth9_icon.png"))
        .unwrap()
        .write_all(b"depth 9")
        .unwrap();

    let res_t8 = resolver.find_icon("depth8_icon", 48, 1, "T0").unwrap();
    assert!(
        res_t8.is_some() && res_t8.unwrap().ends_with("T8/48x48/apps/depth8_icon.png"),
        "Theme at depth 8 should be reachable with MAX_INHERITANCE_DEPTH = 8"
    );

    let res_t9 = resolver.find_icon("depth9_icon", 48, 1, "T0").unwrap();
    assert!(
        res_t9.is_none(),
        "Theme at depth 9 (beyond cap 8) must NOT be reached"
    );

    // 6. Hicolor last: icon in both UserThemeChain and hicolor -> UserThemeChain wins
    create_theme_fixture(
        &root,
        "CustomTheme",
        "[Icon Theme]\nName=CustomTheme\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );
    let dir_custom = root.join("CustomTheme/48x48/apps");
    fs::create_dir_all(&dir_custom).unwrap();
    File::create(dir_custom.join("both_icon.png"))
        .unwrap()
        .write_all(b"from custom")
        .unwrap();
    File::create(dir_hicolor.join("both_icon.png"))
        .unwrap()
        .write_all(b"from hicolor")
        .unwrap();

    let res_both = resolver
        .find_icon("both_icon", 48, 1, "CustomTheme")
        .unwrap()
        .unwrap();
    assert!(
        res_both.ends_with("CustomTheme/48x48/apps/both_icon.png"),
        "CustomTheme must win over hicolor fallback. Resolved: {res_both:?}"
    );
}

#[test]
fn test_config_additional_branches() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("icons");
    fs::create_dir_all(&root).unwrap();

    // 1. read_theme_from_path branches
    let non_existent = tmp.path().join("non_existent.toml");
    assert_eq!(
        kynex_icons::config::read_theme_from_path(&non_existent),
        None
    );

    // Large file > 64 KiB
    let large_file = tmp.path().join("large.toml");
    let mut large_content = String::from("[icons]\ntheme = \"test\"\n");
    large_content.push_str(&"# comment\n".repeat(8000));
    fs::write(&large_file, large_content).unwrap();
    assert_eq!(kynex_icons::config::read_theme_from_path(&large_file), None);

    // Missing [icons] section
    let no_icons = tmp.path().join("no_icons.toml");
    fs::write(&no_icons, "[other]\ntheme = \"foo\"\n").unwrap();
    assert_eq!(kynex_icons::config::read_theme_from_path(&no_icons), None);

    // Missing theme key in [icons]
    let no_theme = tmp.path().join("no_theme.toml");
    fs::write(&no_theme, "[icons]\nother = \"foo\"\n").unwrap();
    assert_eq!(kynex_icons::config::read_theme_from_path(&no_theme), None);

    // Empty theme or whitespace-only
    let empty_theme = tmp.path().join("empty_theme.toml");
    fs::write(&empty_theme, "[icons]\ntheme = \"   \"\n").unwrap();
    assert_eq!(
        kynex_icons::config::read_theme_from_path(&empty_theme),
        None
    );

    // Valid theme with whitespace trim
    let valid_theme = tmp.path().join("valid_theme.toml");
    fs::write(&valid_theme, "[icons]\ntheme = \"  trimmed_theme  \"\n").unwrap();
    assert_eq!(
        kynex_icons::config::read_theme_from_path(&valid_theme),
        Some("trimmed_theme".to_string())
    );

    // 2. resolve_active_theme with default/index.theme system default
    let default_theme_dir = root.join("default");
    fs::create_dir_all(&default_theme_dir).unwrap();
    let mut def_file = File::create(default_theme_dir.join("index.theme")).unwrap();
    def_file
        .write_all(b"[Icon Theme]\nName=Default\nInherits=NonExistentSys,ExistingSys\n")
        .unwrap();

    // ExistingSys exists
    create_theme_fixture(
        &root,
        "ExistingSys",
        "[Icon Theme]\nName=ExistingSys\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\n",
    );

    let roots = IconRoots::custom(vec![root.clone()], vec![], vec![]);
    assert!(kynex_icons::config::theme_exists("ExistingSys", &roots));
    assert!(!kynex_icons::config::theme_exists("NonExistentSys", &roots));

    // Should resolve ExistingSys from system default Inherits
    let resolved_sys = kynex_icons::config::resolve_active_theme(&roots);
    assert_eq!(resolved_sys, "ExistingSys");

    // When default index.theme has no matching theme, falls back to hicolor
    let empty_roots = IconRoots::custom(vec![tmp.path().join("empty_roots")], vec![], vec![]);
    let resolved_hicolor = kynex_icons::config::resolve_active_theme(&empty_roots);
    assert_eq!(resolved_hicolor, kynex_icons::config::HICOLOR_THEME_NAME);
}

#[test]
fn test_roots_additional_branches() {
    let tmp = TempDir::new().unwrap();
    let theme_root = tmp.path().join("icons");
    let pixmap_root = tmp.path().join("pixmaps");
    let data_root = tmp.path().join("share");
    fs::create_dir_all(&theme_root).unwrap();
    fs::create_dir_all(&pixmap_root).unwrap();
    fs::create_dir_all(&data_root).unwrap();

    let roots = IconRoots::custom(
        vec![theme_root.clone()],
        vec![pixmap_root.clone()],
        vec![data_root.clone()],
    );

    assert_eq!(roots.theme_roots(), std::slice::from_ref(&theme_root));
    assert_eq!(roots.pixmap_roots(), std::slice::from_ref(&pixmap_root));

    // validate_and_resolve_file: FileNotFound
    let missing_file = theme_root.join("missing.png");
    match roots.validate_and_resolve_file(&missing_file) {
        Err(kynex_icons::error::IconError::FileNotFound(p)) => assert_eq!(p, missing_file),
        other => panic!("Expected FileNotFound, got {other:?}"),
    }

    // validate_and_resolve_file: NotARegularFile (directory)
    let sub_dir = theme_root.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    match roots.validate_and_resolve_file(&sub_dir) {
        Err(kynex_icons::error::IconError::NotARegularFile(p)) => {
            assert_eq!(p, fs::canonicalize(&sub_dir).unwrap());
        }
        other => panic!("Expected NotARegularFile, got {other:?}"),
    }

    // validate_and_resolve_file: FileSizeExceeded (> 1 MiB)
    let huge_file = theme_root.join("huge.png");
    let f = File::create(&huge_file).unwrap();
    f.set_len(1024 * 1024 + 10).unwrap();
    match roots.validate_and_resolve_file(&huge_file) {
        Err(kynex_icons::error::IconError::FileSizeExceeded { size, limit, .. }) => {
            assert_eq!(size, 1024 * 1024 + 10);
            assert_eq!(limit, 1024 * 1024);
        }
        other => panic!("Expected FileSizeExceeded, got {other:?}"),
    }

    // validate_and_resolve_file: SymlinkEscapesRoot
    let outside_tmp = TempDir::new().unwrap();
    let outside_target = outside_tmp.path().join("secret.png");
    File::create(&outside_target)
        .unwrap()
        .write_all(b"secret")
        .unwrap();
    let escaping_symlink = theme_root.join("escaping.png");
    symlink(&outside_target, &escaping_symlink).unwrap();
    match roots.validate_and_resolve_file(&escaping_symlink) {
        Err(kynex_icons::error::IconError::SymlinkEscapesRoot {
            symlink: s,
            target: t,
        }) => {
            assert_eq!(s, escaping_symlink);
            assert_eq!(t, fs::canonicalize(&outside_target).unwrap());
        }
        other => panic!("Expected SymlinkEscapesRoot, got {other:?}"),
    }

    // validate_absolute_path_icon: Relative path rejected
    match roots.validate_absolute_path_icon(Path::new("relative/path.png")) {
        Err(kynex_icons::error::IconError::AbsolutePathNotAllowed(p)) => {
            assert_eq!(p, PathBuf::from("relative/path.png"));
        }
        other => panic!("Expected AbsolutePathNotAllowed, got {other:?}"),
    }

    // validate_absolute_path_icon: Unsupported extension (.jpg)
    let jpg_file = theme_root.join("test.jpg");
    File::create(&jpg_file).unwrap().write_all(b"jpg").unwrap();
    match roots.validate_absolute_path_icon(&jpg_file) {
        Err(kynex_icons::error::IconError::InvalidIconName { reason, .. }) => {
            assert!(reason.contains(".png or .svg"));
        }
        other => panic!("Expected InvalidIconName, got {other:?}"),
    }

    // validate_absolute_path_icon: Escaping symlink maps to AbsolutePathNotAllowed
    match roots.validate_absolute_path_icon(&escaping_symlink) {
        Err(kynex_icons::error::IconError::AbsolutePathNotAllowed(p)) => {
            assert_eq!(p, escaping_symlink);
        }
        other => panic!("Expected AbsolutePathNotAllowed, got {other:?}"),
    }

    // Confinement check
    assert!(!roots.is_confined(outside_tmp.path()));
    assert!(roots.is_confined(&fs::canonicalize(&theme_root).unwrap()));
}

#[test]
fn test_theme_additional_branches() {
    let tmp = TempDir::new().unwrap();

    // 1. DirectoryType default
    assert_eq!(DirectoryType::default(), DirectoryType::Threshold);

    // 2. SubdirectoryMeta scale mismatch
    let meta_scale2 = SubdirectoryMeta {
        name: "48x48@2/apps".to_string(),
        size: 48,
        scale: 2,
        dir_type: DirectoryType::Fixed,
        min_size: 48,
        max_size: 48,
        threshold: 2,
    };
    assert!(!meta_scale2.matches_size(48, 1));
    assert!(meta_scale2.matches_size(48, 2));

    // 3. SubdirectoryMeta size_distance calculations
    // Fixed type
    let meta_fixed = SubdirectoryMeta {
        name: "48x48/apps".to_string(),
        size: 48,
        scale: 1,
        dir_type: DirectoryType::Fixed,
        min_size: 48,
        max_size: 48,
        threshold: 2,
    };
    assert_eq!(meta_fixed.size_distance(48, 1), 0);
    assert_eq!(meta_fixed.size_distance(32, 1), 16);
    assert_eq!(meta_fixed.size_distance(64, 1), 16);

    // Scalable type
    let meta_scalable = SubdirectoryMeta {
        name: "scalable/apps".to_string(),
        size: 48,
        scale: 1,
        dir_type: DirectoryType::Scalable,
        min_size: 32,
        max_size: 64,
        threshold: 2,
    };
    assert_eq!(meta_scalable.size_distance(48, 1), 0);
    assert_eq!(meta_scalable.size_distance(20, 1), 12); // 32 - 20
    assert_eq!(meta_scalable.size_distance(80, 1), 16); // 80 - 64

    // Threshold type
    let meta_thresh = SubdirectoryMeta {
        name: "48x48/apps".to_string(),
        size: 48,
        scale: 1,
        dir_type: DirectoryType::Threshold,
        min_size: 48,
        max_size: 48,
        threshold: 4,
    };
    assert_eq!(meta_thresh.size_distance(48, 1), 0);
    assert_eq!(meta_thresh.size_distance(40, 1), 4); // target 40 < min 44 -> 4
    assert_eq!(meta_thresh.size_distance(60, 1), 8); // target 60 > max 52 -> 8

    // 4. ThemeIndex::from_file error paths
    let missing_theme = tmp.path().join("missing/index.theme");
    match kynex_icons::theme::ThemeIndex::from_file(&missing_theme) {
        Err(kynex_icons::error::IconError::FileNotFound(p)) => assert_eq!(p, missing_theme),
        other => panic!("Expected FileNotFound, got {other:?}"),
    }

    // Path is directory
    let dir_theme = tmp.path().join("dir_theme");
    fs::create_dir_all(&dir_theme).unwrap();
    match kynex_icons::theme::ThemeIndex::from_file(&dir_theme) {
        Err(kynex_icons::error::IconError::NotARegularFile(p)) => assert_eq!(p, dir_theme),
        other => panic!("Expected NotARegularFile, got {other:?}"),
    }

    // File > 512 KiB
    let huge_theme = tmp.path().join("huge_index.theme");
    let hf = File::create(&huge_theme).unwrap();
    hf.set_len(512 * 1024 + 10).unwrap();
    match kynex_icons::theme::ThemeIndex::from_file(&huge_theme) {
        Err(kynex_icons::error::IconError::FileSizeExceeded { size, limit, .. }) => {
            assert_eq!(size, 512 * 1024 + 10);
            assert_eq!(limit, 512 * 1024);
        }
        other => panic!("Expected FileSizeExceeded, got {other:?}"),
    }

    // 5. ThemeIndex::parse_reader boundary conditions
    // Exceeded maximum line length (> 32 KiB)
    let long_line = format!("[Icon Theme]\nComment={}\n", "x".repeat(33 * 1024));
    let err_long = kynex_icons::theme::ThemeIndex::parse_reader(
        std::io::Cursor::new(long_line.as_bytes()),
        Path::new("test"),
    );
    assert!(err_long.is_err());
    match err_long {
        Err(kynex_icons::error::IconError::InvalidThemeFile { reason, .. }) => {
            assert!(reason.contains("exceeded maximum length"));
        }
        other => panic!("Expected InvalidThemeFile, got {other:?}"),
    }

    // Exceeded maximum line count (> 10,000 lines)
    let many_lines = format!("[Icon Theme]\nName=Test\n{}", "# comment\n".repeat(10_005));
    let err_many = kynex_icons::theme::ThemeIndex::parse_reader(
        std::io::Cursor::new(many_lines.as_bytes()),
        Path::new("test"),
    );
    assert!(err_many.is_err());
    match err_many {
        Err(kynex_icons::error::IconError::InvalidThemeFile { reason, .. }) => {
            assert!(reason.contains("exceeded maximum line count"));
        }
        other => panic!("Expected InvalidThemeFile, got {other:?}"),
    }

    // Missing directory section or invalid sizes
    let theme_text = r"
[Icon Theme]
Name=Comprehensive
Comment=Test Theme
Inherits=Parent1,Parent2
Directories=48x48/apps,missing/section,zero/size,huge/size,scalable/apps
ScaledDirectories=64x64/apps

[48x48/apps]
Size=48
Scale=1
Type=Fixed

[zero/size]
Size=0

[huge/size]
Size=5000

[scalable/apps]
Size=48
Type=Scalable
MinSize=16
MaxSize=128

[64x64/apps]
Size=64
Type=UnknownType
Threshold=3
";
    let parsed = kynex_icons::theme::ThemeIndex::parse_reader(
        std::io::Cursor::new(theme_text.as_bytes()),
        Path::new("test"),
    )
    .unwrap();

    assert_eq!(parsed.name, "Comprehensive");
    assert_eq!(parsed.comment, "Test Theme");
    assert_eq!(parsed.inherits, vec!["Parent1", "Parent2"]);
    // zero/size, huge/size, and missing/section should be skipped
    assert_eq!(parsed.directories.len(), 3);

    let d0 = &parsed.directories[0];
    assert_eq!(d0.name, "48x48/apps");
    assert_eq!(d0.size, 48);
    assert_eq!(d0.dir_type, DirectoryType::Fixed);

    let d1 = &parsed.directories[1];
    assert_eq!(d1.name, "scalable/apps");
    assert_eq!(d1.dir_type, DirectoryType::Scalable);
    assert_eq!(d1.min_size, 16);
    assert_eq!(d1.max_size, 128);

    let d2 = &parsed.directories[2];
    assert_eq!(d2.name, "64x64/apps");
    assert_eq!(d2.dir_type, DirectoryType::Threshold); // Unknown defaults to Threshold
    assert_eq!(d2.threshold, 3);
}
