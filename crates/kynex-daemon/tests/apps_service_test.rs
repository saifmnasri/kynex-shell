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
//! Integration test for `org.kynexos.Daemon.Apps1` service and process isolation.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

use kynex_daemon::services::apps::{AppsServer, AppsSharedState};
use kynex_ipc::apps::{Apps1Proxy, APPS_PATH};
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
async fn test_apps_service_full_lifecycle() {
    let bus = TestBus::new().await.expect("Spawn test dbus");
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    // 1. Create a fake app runner script that dumps its argv and environment to a file
    let dump_file = tmp.path().join("dump.txt");
    let script_path = tmp.path().join("fake_runner.sh");
    let script_content = format!(
        r#"#!/usr/bin/env bash
echo "ARGV_COUNT=$#" > "{dump}"
for arg in "$@"; do
    echo "ARG=$arg" >> "{dump}"
done
echo "=== ENV ===" >> "{dump}"
env | sort >> "{dump}"
"#,
        dump = dump_file.display()
    );
    fs::write(&script_path, script_content).unwrap();
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

    // 2. Create desktop entries in temp directory
    // Normal app
    let normal_desktop = format!(
        r#"[Desktop Entry]
Type=Application
Name=Test Application
GenericName=Testing Tool
Comment=Used for integration testing
Icon=utilities-terminal
Exec={script} "first arg" "; rm -rf /" "$(reboot)" `id`
Categories=Utility;Development;
Keywords=test;tool;
"#,
        script = script_path.display()
    );
    fs::write(app_dir.join("test-app.desktop"), normal_desktop).unwrap();

    // Terminal app
    let terminal_desktop = r#"[Desktop Entry]
Type=Application
Name=Terminal App
Exec=htop
Terminal=true
"#;
    fs::write(app_dir.join("terminal-app.desktop"), terminal_desktop).unwrap();

    // 3. Initialize Shared State and AppsServer
    let shared = Arc::new(AppsSharedState::new(vec![app_dir.clone()], "Kynex"));
    shared.reload().await;
    let server = AppsServer::new(Arc::clone(&shared));

    let conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(APPS_PATH, server)
        .build()
        .await
        .expect("Connection build");

    let emitter = zbus::object_server::SignalEmitter::new(&conn, APPS_PATH).expect("Emitter build");
    let _watcher = kynex_daemon::services::apps::setup_apps_watcher(Arc::clone(&shared), emitter);

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client build");

    let proxy = Apps1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(APPS_PATH)
        .build()
        .await
        .expect("Proxy build");

    // 4. Test GetApps()
    let apps = proxy.get_apps().await.expect("GetApps must succeed");
    assert_eq!(apps.len(), 2);
    let normal_app = apps.iter().find(|a| a.id == "test-app.desktop").unwrap();
    assert_eq!(normal_app.name, "Test Application");
    assert_eq!(normal_app.generic_name, "Testing Tool");
    assert_eq!(normal_app.icon_name, "utilities-terminal");
    assert_eq!(normal_app.categories, vec!["Utility", "Development"]);

    // 5. Test Search()
    let search_results = proxy.search("test", 10).await.expect("Search must succeed");
    assert!(!search_results.is_empty());
    assert_eq!(search_results[0].id, "test-app.desktop");

    // Test Search query > 128 characters rejection
    let long_query = "a".repeat(129);
    let search_err = proxy.search(&long_query, 10).await;
    assert!(search_err.is_err(), "Query > 128 chars must be rejected");

    // 6. Test Launch(id)
    // Non-existent ID: must fail with InvalidArgs
    let launch_nonexistent = proxy.launch("nonexistent.desktop").await;
    assert!(launch_nonexistent.is_err());

    // Terminal app: must fail with "Terminal applications are unsupported yet"
    let launch_term = proxy.launch("terminal-app.desktop").await;
    let err_str = launch_term.unwrap_err().to_string();
    assert!(
        err_str.contains("Terminal applications are unsupported yet"),
        "Unexpected error: {err_str}"
    );

    // Set an adversarial environment variable in the current process
    // To PROVE it gets filtered out by the allowlist!
    std::env::set_var("ADVERSARIAL_SECRET_INJECTION", "SUPER_SECRET_TOKEN");

    // Launch normal fake app
    proxy
        .launch("test-app.desktop")
        .await
        .expect("Launch must succeed");

    // Wait up to 3s for fake runner to write dump file
    let mut dumped = false;
    for _ in 0..30 {
        if dump_file.exists() && fs::metadata(&dump_file).unwrap().len() > 0 {
            dumped = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(dumped, "Fake application dump file must be generated");

    let dump_content = fs::read_to_string(&dump_file).unwrap();

    // PROOF 1: argv contains literal metacharacters without shell interpretation
    assert!(dump_content.contains("ARG=first arg"));
    assert!(dump_content.contains("ARG=; rm -rf /"));
    assert!(dump_content.contains("ARG=$(reboot)"));
    assert!(dump_content.contains("ARG=`id`"));

    // PROOF 2: Environment sanitization (ADVERSARIAL_SECRET_INJECTION is NOT present!)
    assert!(
        !dump_content.contains("ADVERSARIAL_SECRET_INJECTION"),
        "Un-allowlisted environment variable leaked into spawned child!"
    );

    // 7. Test Launch rate limiting (max 5 calls/s)
    let mut rate_limited = false;
    for _ in 0..10 {
        if proxy.launch("test-app.desktop").await.is_err() {
            rate_limited = true;
            break;
        }
    }
    assert!(rate_limited, "Rate limiting must trigger after 5 calls/s");

    drop(conn);
}
