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
//! Watcher debounce, atomic rename, and delete tests.

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use tempfile::tempdir;

use kynex_apps_index::watcher::AppsWatcher;

#[test]
fn test_watcher_burst_coalescing() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let trigger_count = Arc::new(AtomicUsize::new(0));
    let trigger_clone = Arc::clone(&trigger_count);

    let _watcher = AppsWatcher::new(&[app_dir.clone()], move || {
        trigger_clone.fetch_add(1, Ordering::SeqCst);
    })
    .expect("Watcher must initialize");

    // Rapid burst: create 20 files in rapid succession (within 50ms)
    for i in 0..20 {
        let content = format!("[Desktop Entry]\nType=Application\nName=Burst {i}\nExec=cmd\n");
        fs::write(app_dir.join(format!("burst_{i}.desktop")), content).unwrap();
        sleep(Duration::from_millis(2));
    }

    // Wait 350ms for the 200ms quiet window to elapse
    sleep(Duration::from_millis(350));

    // Because of 200ms debouncing, all 20 writes in burst must coalesce into exactly 1 trigger
    assert_eq!(trigger_count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_watcher_atomic_rename() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let trigger_count = Arc::new(AtomicUsize::new(0));
    let trigger_clone = Arc::clone(&trigger_count);

    let _watcher = AppsWatcher::new(&[app_dir.clone()], move || {
        trigger_clone.fetch_add(1, Ordering::SeqCst);
    })
    .expect("Watcher must initialize");

    // Write to a temporary file
    let temp_file = app_dir.join(".app.desktop.tmp123");
    let target_file = app_dir.join("app.desktop");
    fs::write(
        &temp_file,
        b"[Desktop Entry]\nType=Application\nName=Atomic\nExec=cmd\n",
    )
    .unwrap();

    // Atomic rename
    fs::rename(&temp_file, &target_file).unwrap();

    // Wait for debounce
    sleep(Duration::from_millis(350));

    assert!(trigger_count.load(Ordering::SeqCst) >= 1);
}

#[test]
fn test_watcher_deletion() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let file_path = app_dir.join("to_delete.desktop");
    fs::write(
        &file_path,
        b"[Desktop Entry]\nType=Application\nName=Delete Me\nExec=cmd\n",
    )
    .unwrap();

    let trigger_count = Arc::new(AtomicUsize::new(0));
    let trigger_clone = Arc::clone(&trigger_count);

    let _watcher = AppsWatcher::new(&[app_dir], move || {
        trigger_clone.fetch_add(1, Ordering::SeqCst);
    })
    .expect("Watcher must initialize");

    // Delete file
    fs::remove_file(&file_path).unwrap();

    // Wait for debounce
    sleep(Duration::from_millis(350));

    assert_eq!(trigger_count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_watcher_directory_and_nonexistent_handling() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    let nonexistent = tmp.path().join("does_not_exist");
    fs::create_dir_all(&app_dir).unwrap();

    let trigger_count = Arc::new(AtomicUsize::new(0));
    let trigger_clone = Arc::clone(&trigger_count);

    let _watcher = AppsWatcher::new(&[app_dir.clone(), nonexistent], move || {
        trigger_clone.fetch_add(1, Ordering::SeqCst);
    })
    .expect("Watcher must initialize with missing dir");

    // Creating a directory inside watched dir triggers directory check
    let sub = app_dir.join("sub_category");
    fs::create_dir_all(&sub).unwrap();
    sleep(Duration::from_millis(350));

    assert_eq!(trigger_count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_watcher_ignores_non_desktop_files_and_clean_drop() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let trigger_count = Arc::new(AtomicUsize::new(0));
    let trigger_clone = Arc::clone(&trigger_count);

    let watcher = AppsWatcher::new(&[app_dir.clone()], move || {
        trigger_clone.fetch_add(1, Ordering::SeqCst);
    })
    .expect("Watcher must initialize");

    // Write a .txt file: should NOT trigger callback
    let txt_file = app_dir.join("notes.txt");
    fs::write(&txt_file, b"random text").unwrap();
    sleep(Duration::from_millis(300));
    assert_eq!(trigger_count.load(Ordering::SeqCst), 0);

    // Write a file with .desktop in name: should trigger callback
    let tmp_desktop = app_dir.join("app.desktop.tmp_123");
    fs::write(&tmp_desktop, b"[Desktop Entry]\nName=Tmp\n").unwrap();
    sleep(Duration::from_millis(350));
    assert_eq!(trigger_count.load(Ordering::SeqCst), 1);

    // Explicit drop should cleanly terminate the worker thread without panic
    drop(watcher);
}
