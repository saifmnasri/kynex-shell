#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::cloned_ref_to_slice_refs,
    clippy::assert_is_empty,
    clippy::single_char_pattern,
    clippy::cast_possible_truncation
)]
//! Precedence, directory scanning, and symlink security tests.

use std::fs;
use tempfile::tempdir;

use kynex_apps_index::index::AppsIndex;

#[test]
fn test_precedence_user_overrides_system() {
    let user_tmp = tempdir().unwrap();
    let sys_tmp = tempdir().unwrap();

    let user_app_dir = user_tmp.path().join("applications");
    let sys_app_dir = sys_tmp.path().join("applications");
    fs::create_dir_all(&user_app_dir).unwrap();
    fs::create_dir_all(&sys_app_dir).unwrap();

    // System file
    let sys_content = b"[Desktop Entry]\nType=Application\nName=System App\nExec=sys\n";
    fs::write(sys_app_dir.join("app.desktop"), sys_content).unwrap();

    // User file overriding system file
    let user_content = b"[Desktop Entry]\nType=Application\nName=User Custom App\nExec=user\n";
    fs::write(user_app_dir.join("app.desktop"), user_content).unwrap();

    let dirs = vec![user_app_dir, sys_app_dir];
    let index = AppsIndex::load_from_dirs(&dirs, "Kynex").unwrap();

    assert_eq!(index.len(), 1);
    let app = index.get("app.desktop").expect("app.desktop must exist");
    assert_eq!(app.name.resolve(None), "User Custom App");
}

#[test]
fn test_user_hidden_masks_system_entry() {
    let user_tmp = tempdir().unwrap();
    let sys_tmp = tempdir().unwrap();

    let user_app_dir = user_tmp.path().join("applications");
    let sys_app_dir = sys_tmp.path().join("applications");
    fs::create_dir_all(&user_app_dir).unwrap();
    fs::create_dir_all(&sys_app_dir).unwrap();

    // System file
    let sys_content = b"[Desktop Entry]\nType=Application\nName=System App\nExec=sys\n";
    fs::write(sys_app_dir.join("app.desktop"), sys_content).unwrap();

    // User file with Hidden=true
    let user_content =
        b"[Desktop Entry]\nType=Application\nName=Deleted App\nExec=sys\nHidden=true\n";
    fs::write(user_app_dir.join("app.desktop"), user_content).unwrap();

    let dirs = vec![user_app_dir, sys_app_dir];
    let index = AppsIndex::load_from_dirs(&dirs, "Kynex").unwrap();

    // app.desktop should be completely masked/absent
    assert_eq!(index.len(), 0);
    assert!(index.get("app.desktop").is_none());
}

#[test]
fn test_subdirectory_desktop_id_generation() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    let sub_dir = app_dir.join("kde").join("system");
    fs::create_dir_all(&sub_dir).unwrap();

    let content = b"[Desktop Entry]\nType=Application\nName=KDE Konsole\nExec=konsole\n";
    fs::write(sub_dir.join("konsole.desktop"), content).unwrap();

    let index = AppsIndex::load_from_dirs(&[app_dir], "Kynex").unwrap();
    assert_eq!(index.len(), 1);
    let app = index
        .get("kde-system-konsole.desktop")
        .expect("Hyphenated ID expected");
    assert_eq!(app.name.resolve(None), "KDE Konsole");
}

#[test]
fn test_symlink_loop_handling() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let loop1 = app_dir.join("loop1");
    let loop2 = app_dir.join("loop2");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(&loop2, &loop1);
        let _ = symlink(&loop1, &loop2);

        // Also valid app
        let content = b"[Desktop Entry]\nType=Application\nName=Valid App\nExec=app\n";
        fs::write(app_dir.join("valid.desktop"), content).unwrap();

        // Loading should terminate cleanly and not hang or loop infinitely
        let index = AppsIndex::load_from_dirs(&[app_dir], "Kynex").unwrap();
        assert_eq!(index.len(), 1);
        assert!(index.get("valid.desktop").is_some());
    }
}

#[test]
fn test_10000_files_benchmark_and_stability() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    for i in 0..10_000 {
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=App {i}\nExec=bin_{i}\nCategories=Tools;\n"
        );
        fs::write(app_dir.join(format!("app_{i}.desktop")), content).unwrap();
    }

    let start = std::time::Instant::now();
    let index = AppsIndex::load_from_dirs(&[app_dir], "Kynex").unwrap();
    let elapsed = start.elapsed();

    assert_eq!(index.len(), 10_000);
    // 10,000 files should index well within a reasonable timeframe (e.g. < 500 ms)
    assert!(elapsed < std::time::Duration::from_millis(1500));
}

#[test]
fn test_index_standard_dirs_and_methods() {
    let empty_index = AppsIndex::new("Kynex");
    assert!(empty_index.is_empty());
    assert_eq!(empty_index.current_desktop(), "Kynex");
    assert_eq!(empty_index.entries().len(), 0);
    assert!(empty_index.get("nonexistent.desktop").is_none());
    let _ = format!("{empty_index:?}");
    let _ = empty_index.clone();
    let _ = AppsIndex::default();

    // Test XDG_DATA_HOME env branch
    std::env::set_var("XDG_DATA_HOME", "/tmp/nonexistent_test_xdg_home");
    let dirs = AppsIndex::standard_app_dirs();
    assert!(!dirs.is_empty());
    assert!(dirs
        .iter()
        .any(|d| d.to_string_lossy().contains("nonexistent_test_xdg_home")));
    std::env::remove_var("XDG_DATA_HOME");

    // Non-existent directory handling in load_from_dirs
    let non_dir_res = AppsIndex::load_from_dirs(
        &[std::path::PathBuf::from("/nonexistent/path/never/exists")],
        "Kynex",
    )
    .unwrap();
    assert!(non_dir_res.is_empty());

    // Load standard index
    let standard = AppsIndex::load_standard("Kynex").expect("Standard load should succeed");
    assert_eq!(standard.current_desktop(), "Kynex");
    let results = standard.search("term", 5, None);
    // Search method executed
    let _ = results.len();
}

#[test]
fn test_index_incremental_update_and_removal() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let file_path = app_dir.join("live.desktop");
    let initial_content = b"[Desktop Entry]\nType=Application\nName=Initial App\nExec=init\n";
    fs::write(&file_path, initial_content).unwrap();

    let mut index = AppsIndex::load_from_dirs(&[app_dir], "Kynex").unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(
        index.get("live.desktop").unwrap().name.resolve(None),
        "Initial App"
    );

    // 1. Update file in place
    let updated_content = b"[Desktop Entry]\nType=Application\nName=Updated App\nExec=upd\n";
    fs::write(&file_path, updated_content).unwrap();
    index.update_entry_file("live.desktop", &file_path);
    assert_eq!(index.len(), 1);
    assert_eq!(
        index.get("live.desktop").unwrap().name.resolve(None),
        "Updated App"
    );

    // 2. Add brand new file
    let new_path = file_path.parent().unwrap().join("second.desktop");
    fs::write(
        &new_path,
        b"[Desktop Entry]\nType=Application\nName=Second App\nExec=sec\n",
    )
    .unwrap();
    index.update_entry_file("second.desktop", &new_path);
    assert_eq!(index.len(), 2);
    assert_eq!(
        index.get("second.desktop").unwrap().name.resolve(None),
        "Second App"
    );

    // 3. Make second file invalid (e.g. Hidden=true)
    fs::write(
        &new_path,
        b"[Desktop Entry]\nType=Application\nName=Second App\nExec=sec\nHidden=true\n",
    )
    .unwrap();
    index.update_entry_file("second.desktop", &new_path);
    assert_eq!(index.len(), 1);
    assert!(index.get("second.desktop").is_none());

    // 4. Delete initial file
    fs::remove_file(&file_path).unwrap();
    index.update_entry_file("live.desktop", &file_path);
    assert_eq!(index.len(), 0);
    assert!(index.is_empty());
}

#[test]
fn test_precedence_masking_nodisplay_and_desktop_not_allowed() {
    let user_tmp = tempdir().unwrap();
    let sys_tmp = tempdir().unwrap();

    let user_app_dir = user_tmp.path().join("applications");
    let sys_app_dir = sys_tmp.path().join("applications");
    fs::create_dir_all(&user_app_dir).unwrap();
    fs::create_dir_all(&sys_app_dir).unwrap();

    // System files
    fs::write(
        sys_app_dir.join("app1.desktop"),
        b"[Desktop Entry]\nType=Application\nName=Sys 1\nExec=s1\n",
    )
    .unwrap();
    fs::write(
        sys_app_dir.join("app2.desktop"),
        b"[Desktop Entry]\nType=Application\nName=Sys 2\nExec=s2\n",
    )
    .unwrap();
    fs::write(
        sys_app_dir.join("oversized.desktop"),
        b"[Desktop Entry]\nType=Application\nName=Sys 3\nExec=s3\n",
    )
    .unwrap();

    // User files overriding with NoDisplay and OnlyShowIn=GNOME
    fs::write(
        user_app_dir.join("app1.desktop"),
        b"[Desktop Entry]\nType=Application\nName=User 1\nExec=s1\nNoDisplay=true\n",
    )
    .unwrap();
    fs::write(
        user_app_dir.join("app2.desktop"),
        b"[Desktop Entry]\nType=Application\nName=User 2\nExec=s2\nOnlyShowIn=GNOME;\n",
    )
    .unwrap();
    // Oversized in user dir (> 64 KiB)
    let big = vec![b'a'; 70 * 1024];
    fs::write(user_app_dir.join("oversized.desktop"), &big).unwrap();

    // Directory ending in .desktop
    fs::create_dir_all(user_app_dir.join("fake.desktop")).unwrap();

    let dirs = vec![user_app_dir, sys_app_dir];
    let index = AppsIndex::load_from_dirs(&dirs, "Kynex").unwrap();

    // None of these should appear in index
    assert_eq!(index.len(), 0);
}
