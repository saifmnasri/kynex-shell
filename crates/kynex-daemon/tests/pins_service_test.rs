#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::cloned_ref_to_slice_refs,
    clippy::assert_is_empty,
    clippy::single_char_pattern,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]
//! Integration tests for `org.kynexos.Daemon.Pins1` service (M2.5).

use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

use kynex_daemon::services::pins::{PinnedFileSchema, PinsServer, PinsSharedState, MAX_PINS};
use kynex_ipc::pins::{Pins1Proxy, PINS_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use tokio::io::AsyncBufReadExt;

/// Isolated private D-Bus daemon helper.
struct TestBus {
    child: tokio::process::Child,
    address: String,
}

impl TestBus {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let mut cmd = tokio::process::Command::new("dbus-daemon");
        cmd.arg("--session")
            .arg("--print-address=1")
            .arg("--nofork")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture dbus-daemon stdout")?;
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();

        tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await??;
        let address = line.trim().to_string();

        Ok(Self { child, address })
    }

    fn address(&self) -> &str {
        &self.address
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

#[tokio::test]
async fn test_pins_service_pin_unpin_move_and_permissions() {
    let bus = TestBus::new().await.unwrap();
    let tmp = tempdir().unwrap();
    let config_path = tmp.path().join("pinned.toml");

    let shared = Arc::new(PinsSharedState::new(config_path.clone()));
    let server = PinsServer::new(shared.clone());

    let conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(PINS_PATH, server)
        .build()
        .await
        .unwrap();

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .unwrap();

    let proxy = Pins1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(PINS_PATH)
        .build()
        .await
        .unwrap();

    // 1. Initial pins empty
    let initial = proxy.get().await.unwrap();
    assert_eq!(initial, Vec::<String>::new());

    // 2. Pin 3 apps (note rate limit: 10/s)
    proxy.pin("app-a.desktop").await.unwrap();
    proxy.pin("app-b.desktop").await.unwrap();
    proxy.pin("app-c.desktop").await.unwrap();

    let current = proxy.get().await.unwrap();
    assert_eq!(
        current,
        vec!["app-a.desktop", "app-b.desktop", "app-c.desktop"]
    );

    // 3. Move app-c to index 0
    proxy.move_pin("app-c.desktop", 0).await.unwrap();
    let moved = proxy.get().await.unwrap();
    assert_eq!(
        moved,
        vec!["app-c.desktop", "app-a.desktop", "app-b.desktop"]
    );

    // 4. Unpin app-a
    proxy.unpin("app-a.desktop").await.unwrap();
    let unpinned = proxy.get().await.unwrap();
    assert_eq!(unpinned, vec!["app-c.desktop", "app-b.desktop"]);

    // 5. Verify file permissions: mode must be 0600
    let metadata = fs::metadata(&config_path).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "pinned.toml must have 0600 permissions");

    drop(conn);
}

#[tokio::test]
async fn test_pins_service_corrupt_file_quarantine() {
    let tmp = tempdir().unwrap();
    let config_path = tmp.path().join("pinned.toml");

    // Write corrupted non-TOML garbage
    let mut file = File::create(&config_path).unwrap();
    file.write_all(b"<<<<< INVALID TOML GARBAGE >>>>>\n")
        .unwrap();
    drop(file);

    // Initializing state should quarantine file to .bad without panic
    let shared = Arc::new(PinsSharedState::new(config_path.clone()));
    let pins = shared.get_pins().await;
    assert_eq!(
        pins,
        Vec::<String>::new(),
        "Corrupt file must yield empty pins"
    );

    // Verify .bad quarantine file exists
    let parent = config_path.parent().unwrap();
    let entries = fs::read_dir(parent).unwrap();
    let bad_found = entries
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().contains(".bad"));
    assert!(bad_found, "Corrupt file must be renamed to .bad.*");
}

#[tokio::test]
async fn test_pins_service_max_limit_enforced() {
    let tmp = tempdir().unwrap();
    let config_path = tmp.path().join("pinned.toml");

    let _shared = Arc::new(PinsSharedState::new(config_path.clone()));

    // Populate directly up to MAX_PINS
    {
        let mut initial_pins = Vec::new();
        for i in 0..MAX_PINS {
            initial_pins.push(format!("app_{i}.desktop"));
        }
        let schema = PinnedFileSchema {
            schema_version: 1,
            pins: initial_pins,
        };
        let toml_data = toml::to_string_pretty(&schema).unwrap();
        fs::write(&config_path, toml_data).unwrap();
    }

    let shared = Arc::new(PinsSharedState::new(config_path.clone()));
    assert_eq!(shared.get_pins().await.len(), MAX_PINS);

    let bus = TestBus::new().await.unwrap();
    let conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(PINS_PATH, PinsServer::new(shared))
        .build()
        .await
        .unwrap();

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .unwrap();

    let proxy = Pins1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(PINS_PATH)
        .build()
        .await
        .unwrap();

    // Attempting to pin the 129th app must fail with LimitsExceeded
    let res = proxy.pin("excess_app.desktop").await;
    assert!(res.is_err(), "Must reject pins exceeding MAX_PINS limit");

    drop(conn);
}

#[tokio::test]
async fn test_pins_service_symlink_swap_safety() {
    let tmp = tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let target_dir = tmp.path().join("target");
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    let victim_file = target_dir.join("victim.txt");
    fs::write(&victim_file, b"untouched victim content").unwrap();

    let config_path = config_dir.join("pinned.toml");
    std::os::unix::fs::symlink(&victim_file, &config_path).unwrap();

    let shared = Arc::new(PinsSharedState::new(config_path.clone()));
    // Persisting must replace the symlink atomically using rename
    shared.persist().await.unwrap();

    // Verify victim content was NOT overwritten through symlink
    let victim_content = fs::read_to_string(&victim_file).unwrap();
    assert_eq!(
        victim_content, "untouched victim content",
        "Symlink target must not be overwritten"
    );

    // Config path must now be a regular file with 0600 mode
    let meta = fs::symlink_metadata(&config_path).unwrap();
    assert!(
        meta.is_file(),
        "Replaced path must be a regular file, not a symlink"
    );
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
}

#[tokio::test]
async fn test_pins_atomic_write_crash_resilience() {
    let tmp = tempdir().unwrap();
    let config_path = tmp.path().join("pinned.toml");

    // Initial valid state
    let schema = PinnedFileSchema {
        schema_version: 1,
        pins: vec!["existing_app.desktop".to_string()],
    };
    fs::write(&config_path, toml::to_string_pretty(&schema).unwrap()).unwrap();

    let shared = Arc::new(PinsSharedState::new(config_path.clone()));

    // Simulate leftover temporary file (e.g. from SIGKILL prior to rename)
    let parent = config_path.parent().unwrap();
    let abandoned_temp = parent.join(".pinned.toml.tmp.99999.0");
    fs::write(&abandoned_temp, b"incomplete partial write").unwrap();

    // Reading or persisting cleans up and leaves config_path valid
    let pins = shared.get_pins().await;
    assert_eq!(pins, vec!["existing_app.desktop"]);

    // Original file remains intact and valid TOML
    let file_content = fs::read_to_string(&config_path).unwrap();
    assert!(file_content.contains("existing_app.desktop"));
}

#[tokio::test]
async fn test_pins_concurrent_mutations_from_10_connections() {
    let bus = TestBus::new().await.unwrap();
    let tmp = tempdir().unwrap();
    let config_path = tmp.path().join("pinned.toml");

    let shared = Arc::new(PinsSharedState::new(config_path.clone()));
    let server = PinsServer::new(shared.clone());

    let _conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(PINS_PATH, server)
        .build()
        .await
        .unwrap();

    let bus_addr = bus.address().to_string();
    let mut handles = Vec::new();
    for i in 0..10 {
        let addr = bus_addr.clone();
        let app_id = format!("concurrent_app_{i}.desktop");
        let handle = tokio::spawn(async move {
            let client_conn = zbus::connection::Builder::address(addr.as_str())
                .build()
                .await
                .unwrap();
            let proxy = Pins1Proxy::builder(&client_conn)
                .destination(DAEMON_DESTINATION)
                .path(PINS_PATH)
                .build()
                .await
                .unwrap();

            proxy.pin(&app_id).await
        });
        handles.push(handle);
    }

    let mut success_count = 0;
    for handle in handles {
        if let Ok(Ok(())) = handle.await {
            success_count += 1;
        }
    }

    // All 10 requests should complete cleanly (either succeeded or throttled by 10/s rate limit without panic or corrupt state)
    assert!(
        success_count > 0,
        "At least some concurrent pins should succeed"
    );

    // Verify pinned.toml is valid and uncorrupted
    let pins = shared.get_pins().await;
    assert_eq!(pins.len(), success_count);
}
