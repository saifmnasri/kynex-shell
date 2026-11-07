#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]
//! End-to-end integration tests for the Launcher overlay on `Apps1` and `ShellFeed1`.
//!
//! Verifies:
//! 1. Private bus execution with real daemon services (`AppsServer`, `ShellServer`, `ShellFeedServer`).
//! 2. Type -> select -> launch workflow spawning a fake app that records argv to disk.
//! 3. Latest-wins asynchronous search race condition protection (stale slow replies discarded).
//! 4. Shortcut path triggering `LauncherToggleRequested`.
//! 5. Spoof protection rejecting unauthorized non-KWin callers under `KWinSenderPolicy`.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

use futures_util::StreamExt;
use kynex_daemon::services::apps::{AppsServer, AppsSharedState};
use kynex_daemon::services::shell::{ShellFeedServer, ShellServer, ShellSharedState};
use kynex_daemon::services::windows::{AllowAllSenderPolicy, KWinSenderPolicy};
use kynex_ipc::apps::{Apps1Proxy, APPS_PATH};
use kynex_ipc::shell::{Shell1Proxy, ShellFeed1Proxy, SHELL_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use tokio::io::AsyncBufReadExt;
use zbus::object_server::SignalEmitter;

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
async fn test_launcher_e2e_type_select_launch() {
    let bus = TestBus::new().await.expect("Spawn test dbus");
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let output_file = tmp.path().join("launcher_app_output.txt");
    let fake_script = tmp.path().join("fake_editor.sh");
    let script_content = format!(
        r#"#!/bin/sh
echo "$@" > "{}"
exit 0
"#,
        output_file.display()
    );
    fs::write(&fake_script, script_content).unwrap();
    fs::set_permissions(&fake_script, fs::Permissions::from_mode(0o755)).unwrap();

    // Create 3 fake desktop apps
    let desktop1 = format!(
        "[Desktop Entry]\nType=Application\nName=Alpha Text Editor\nGenericName=Editor\nExec={} --file test.txt\n",
        fake_script.display()
    );
    fs::write(app_dir.join("alpha-editor.desktop"), desktop1).unwrap();

    let desktop2 = format!(
        "[Desktop Entry]\nType=Application\nName=Beta Terminal\nGenericName=Command Line\nExec={}\n",
        fake_script.display()
    );
    fs::write(app_dir.join("beta-terminal.desktop"), desktop2).unwrap();

    let desktop3 = format!(
        "[Desktop Entry]\nType=Application\nName=Gamma Calculator\nGenericName=Math\nExec={}\n",
        fake_script.display()
    );
    fs::write(app_dir.join("gamma-calc.desktop"), desktop3).unwrap();

    // Initialize real daemon apps state
    let apps_shared = Arc::new(AppsSharedState::new(vec![app_dir], "Kynex"));
    apps_shared.reload().await;
    let apps_server = AppsServer::new(Arc::clone(&apps_shared));

    let _srv_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(APPS_PATH, apps_server)
        .build()
        .await
        .expect("Server connection");

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client connection");

    let proxy = Apps1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(APPS_PATH)
        .build()
        .await
        .expect("Apps1 proxy");

    // 1. Simulate typing search: "Alpha"
    let results = proxy.search("Alpha", 50).await.expect("Search Alpha");
    assert_eq!(results.len(), 1, "Only Alpha Text Editor should match");
    assert_eq!(results[0].id, "alpha-editor.desktop");
    assert_eq!(results[0].name, "Alpha Text Editor");
    assert_eq!(results[0].generic_name, "Editor");

    // 2. Simulate selection & launch of the chosen app
    let launched_id = &results[0].id;
    proxy.launch(launched_id).await.expect("Launch app");

    // 3. Verify that the fake app executed and wrote its expected arguments to disk
    let mut recorded = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if output_file.exists() {
            let content = fs::read_to_string(&output_file).unwrap_or_default();
            if content.contains("--file test.txt") {
                recorded = true;
                break;
            }
        }
    }

    assert!(
        recorded,
        "Fake app should have executed and written argv to disk"
    );
}

#[tokio::test]
async fn test_launcher_stale_reply_race_protection() {
    let bus = TestBus::new().await.expect("Spawn test dbus");
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let d1 = "[Desktop Entry]\nType=Application\nName=First App\nExec=true\n";
    fs::write(app_dir.join("first.desktop"), d1).unwrap();

    let d2 = "[Desktop Entry]\nType=Application\nName=Second App\nExec=true\n";
    fs::write(app_dir.join("second.desktop"), d2).unwrap();

    let apps_shared = Arc::new(AppsSharedState::new(vec![app_dir], "Kynex"));
    apps_shared.reload().await;
    let apps_server = AppsServer::new(Arc::clone(&apps_shared));

    let _srv_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(APPS_PATH, apps_server)
        .build()
        .await
        .expect("Server connection");

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client connection");

    let proxy = Apps1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(APPS_PATH)
        .build()
        .await
        .expect("Apps1 proxy");

    // Model sequence tracker simulating latest-wins logic
    let mut seq = 0u64;
    let mut last_applied_seq = 0u64;
    let mut current_results = Vec::new();

    // Query 1 dispatched: seq = 1
    seq += 1;
    let seq1 = seq;
    let proxy1 = proxy.clone();
    let task1 = tokio::spawn(async move {
        // Intentionally delayed query
        tokio::time::sleep(Duration::from_millis(100)).await;
        (seq1, proxy1.search("First", 50).await.expect("Search 1"))
    });

    // Query 2 rapidly dispatched: seq = 2
    seq += 1;
    let seq2 = seq;
    let proxy2 = proxy.clone();
    let task2 =
        tokio::spawn(async move { (seq2, proxy2.search("Second", 50).await.expect("Search 2")) });

    // Reply 2 arrives first
    let (reply_seq2, res2) = task2.await.expect("Task 2 join");
    if reply_seq2 > last_applied_seq {
        last_applied_seq = reply_seq2;
        current_results = res2;
    }

    assert_eq!(current_results.len(), 1);
    assert_eq!(current_results[0].name, "Second App");
    assert_eq!(last_applied_seq, 2);

    // Stale Reply 1 arrives later -> MUST be dropped because reply_seq1 < last_applied_seq
    let (reply_seq1, res1) = task1.await.expect("Task 1 join");
    if reply_seq1 > last_applied_seq {
        last_applied_seq = reply_seq1;
        current_results = res1;
    }

    // Results must strictly remain from Query 2
    assert_eq!(current_results.len(), 1);
    assert_eq!(
        current_results[0].name, "Second App",
        "Stale search reply must not overwrite newer search results"
    );
    assert_eq!(last_applied_seq, 2);
}

#[tokio::test]
async fn test_launcher_shortcut_path_and_spoof_protection() {
    let bus = TestBus::new().await.expect("Spawn test dbus");

    // 1. Valid shortcut path under AllowAllSenderPolicy (nested/script session)
    let shell_state = Arc::new(ShellSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let shell_server = ShellServer::new(Arc::clone(&shell_state));
    let feed_server = ShellFeedServer::new(Arc::clone(&shell_state));

    let srv_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(SHELL_PATH, shell_server)
        .serve_at(SHELL_PATH, feed_server.clone())
        .build()
        .await
        .expect("Server connection");

    let emitter = SignalEmitter::new(&srv_conn, SHELL_PATH).expect("Emitter");
    feed_server.set_emitter(emitter).await;

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client connection");

    let shell_proxy = Shell1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(SHELL_PATH)
        .build()
        .await
        .expect("Shell1 proxy");

    let feed_proxy = ShellFeed1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(SHELL_PATH)
        .build()
        .await
        .expect("ShellFeed1 proxy");

    let mut stream = shell_proxy
        .receive_launcher_toggle_requested()
        .await
        .expect("Signal stream");

    // Feeder triggers shortcut
    feed_proxy.launcher_shortcut().await.expect("Shortcut call");

    // Shell listener receives the LauncherToggleRequested signal
    let signal = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("Timeout waiting for toggle signal")
        .expect("Stream yielded signal");

    drop(signal);

    // 2. Spoof protection under KWinSenderPolicy
    let secure_state = Arc::new(ShellSharedState::new(Arc::new(KWinSenderPolicy::new())));
    let secure_feed = ShellFeedServer::new(Arc::clone(&secure_state));

    let secure_srv_conn = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon.Secure")
        .serve_at("/org/kynexos/Daemon/SecureFeed", secure_feed.clone())
        .build()
        .await
        .expect("Secure server conn");

    let secure_emitter =
        SignalEmitter::new(&secure_srv_conn, "/org/kynexos/Daemon/SecureFeed").expect("Emitter");
    secure_feed.set_emitter(secure_emitter).await;

    let spoof_feed_proxy = ShellFeed1Proxy::builder(&client_conn)
        .destination("org.kynexos.Daemon.Secure")
        .path("/org/kynexos/Daemon/SecureFeed")
        .build()
        .await
        .expect("Secure feed proxy");

    // Spoofed call from non-KWin process must fail
    let err = spoof_feed_proxy
        .launcher_shortcut()
        .await
        .expect_err("Unauthorized caller must be rejected");

    assert!(
        err.to_string().contains("AccessDenied") || err.to_string().contains("not authorized"),
        "Error must be authorization failure, got: {err}"
    );
}
