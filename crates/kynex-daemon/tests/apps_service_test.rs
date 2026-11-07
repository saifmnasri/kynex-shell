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

use futures_util::StreamExt;
use kynex_daemon::services::apps::{
    build_clean_environment, setup_apps_watcher, spawn_detached, AppsServer, AppsSharedState,
};
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

#[tokio::test]
async fn test_apps_watcher_service_level_integration() {
    let bus = TestBus::new().await.expect("Spawn test dbus");
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

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
    let _watcher = setup_apps_watcher(Arc::clone(&shared), emitter);

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

    let mut changed_stream = proxy
        .receive_changed()
        .await
        .expect("receive_changed stream");

    // 1. Create a .desktop file -> GetApps() reflects, Changed() emitted
    let live_file = app_dir.join("live-app.desktop");
    fs::write(
        &live_file,
        "[Desktop Entry]\nType=Application\nName=Live App 1\nExec=true\n",
    )
    .unwrap();

    let sig = tokio::time::timeout(Duration::from_millis(2000), changed_stream.next()).await;
    assert!(sig.is_ok(), "Must receive Changed signal on file creation");
    let apps = proxy.get_apps().await.unwrap();
    assert!(
        apps.iter()
            .any(|a| a.id == "live-app.desktop" && a.name == "Live App 1"),
        "GetApps() must reflect created .desktop file"
    );

    // 2. Modify the .desktop file -> GetApps() reflects, Changed() emitted
    fs::write(
        &live_file,
        "[Desktop Entry]\nType=Application\nName=Live App Modified\nExec=true\n",
    )
    .unwrap();

    let sig = tokio::time::timeout(Duration::from_millis(2000), changed_stream.next()).await;
    assert!(
        sig.is_ok(),
        "Must receive Changed signal on file modification"
    );
    let apps = proxy.get_apps().await.unwrap();
    assert!(
        apps.iter()
            .any(|a| a.id == "live-app.desktop" && a.name == "Live App Modified"),
        "GetApps() must reflect modified .desktop file"
    );

    // 3. Rename (atomic save: write .tmp then rename) -> GetApps() reflects, Changed() emitted
    let tmp_file = app_dir.join("live-app.desktop.tmp");
    fs::write(
        &tmp_file,
        "[Desktop Entry]\nType=Application\nName=Live App Atomic\nExec=true\n",
    )
    .unwrap();
    fs::rename(&tmp_file, &live_file).unwrap();

    let sig = tokio::time::timeout(Duration::from_millis(2000), changed_stream.next()).await;
    assert!(sig.is_ok(), "Must receive Changed signal on atomic rename");
    let apps = proxy.get_apps().await.unwrap();
    assert!(
        apps.iter()
            .any(|a| a.id == "live-app.desktop" && a.name == "Live App Atomic"),
        "GetApps() must reflect atomic save rename"
    );

    // 4. Delete the .desktop file -> GetApps() reflects, Changed() emitted
    fs::remove_file(&live_file).unwrap();

    let sig = tokio::time::timeout(Duration::from_millis(2000), changed_stream.next()).await;
    assert!(sig.is_ok(), "Must receive Changed signal on file deletion");
    let apps = proxy.get_apps().await.unwrap();
    assert!(
        !apps.iter().any(|a| a.id == "live-app.desktop"),
        "GetApps() must reflect deleted .desktop file"
    );

    // 5. Burst of 1,000 file operations -> exactly ONE Changed() signal observed per burst
    for i in 0..1000 {
        let name = format!("burst-{}.desktop", i % 25);
        let content = format!("[Desktop Entry]\nType=Application\nName=Burst {i}\nExec=true\n");
        fs::write(app_dir.join(&name), content).unwrap();
    }

    let burst_sig = tokio::time::timeout(Duration::from_millis(3000), changed_stream.next()).await;
    assert!(burst_sig.is_ok(), "Must receive Changed signal for burst");

    // Wait 500ms (well past the 200ms debounce quiet period) to assert NO additional signals arrive
    let extra_sig = tokio::time::timeout(Duration::from_millis(500), changed_stream.next()).await;
    assert!(
        extra_sig.is_err(),
        "Exactly ONE Changed signal must be observed per burst (no extraneous signals)"
    );
}

#[tokio::test]
async fn test_child_process_zombie_reaping_regression() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let shared = Arc::new(AppsSharedState::new(vec![app_dir], "Kynex"));
    let env_allowlist = build_clean_environment();

    let script = tmp.path().join("quick_exit.sh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let argv = vec![script.to_string_lossy().to_string()];
    for _ in 0..50 {
        spawn_detached(&argv, &env_allowlist, shared.tracked_children()).expect("spawn child");
    }

    // Wait 1 second for all 50 children to exit and be reaped
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify zero zombie children of the current process exist
    let my_pid = std::process::id();
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg("ps -eo stat,pid,ppid,cmd")
        .output()
        .expect("ps");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let my_zombies: Vec<&str> = stdout
        .lines()
        .filter(|l| {
            let mut parts = l.split_whitespace();
            let stat = parts.next().unwrap_or("");
            let _pid = parts.next().unwrap_or("");
            let ppid = parts.next().unwrap_or("");
            stat.starts_with('Z') && ppid == my_pid.to_string()
        })
        .collect();

    assert!(
        my_zombies.is_empty(),
        "Expected zero zombie children after 1 s, found: {my_zombies:?}"
    );
}

#[tokio::test]
async fn test_200_long_lived_children_zero_zombies_regression() {
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let shared = Arc::new(AppsSharedState::new(vec![app_dir], "Kynex"));
    let env_allowlist = build_clean_environment();

    let trigger = tmp.path().join("exit_trigger");
    let script = tmp.path().join("wait_and_exit.sh");
    let script_content = format!(
        r#"#!/bin/sh
while [ ! -f "{trigger}" ]; do
    sleep 0.02
done
exit 0
"#,
        trigger = trigger.display()
    );
    fs::write(&script, script_content).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let argv = vec![script.to_string_lossy().to_string()];
    for _ in 0..200 {
        spawn_detached(&argv, &env_allowlist, shared.tracked_children()).expect("spawn child");
    }

    // Assert that 200 children are actively tracked and running
    assert_eq!(
        shared.tracked_children_count(),
        200,
        "All 200 children must be tracked"
    );

    // Trigger all 200 children to exit
    fs::write(&trigger, "exit\n").unwrap();

    // Wait up to 1 second for all children to exit and be reaped
    let mut reaped = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if shared.tracked_children_count() == 0 {
            reaped = true;
            break;
        }
    }
    assert!(reaped, "All 200 children must be reaped within 1s");

    // Verify zero zombie children exist
    let my_pid = std::process::id();
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg("ps -eo stat,pid,ppid,cmd")
        .output()
        .expect("ps");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let my_zombies: Vec<&str> = stdout
        .lines()
        .filter(|l| {
            let mut parts = l.split_whitespace();
            let stat = parts.next().unwrap_or("");
            let _pid = parts.next().unwrap_or("");
            let ppid = parts.next().unwrap_or("");
            stat.starts_with('Z') && ppid == my_pid.to_string()
        })
        .collect();

    assert!(
        my_zombies.is_empty(),
        "Expected zero zombie children after exit, found: {my_zombies:?}"
    );
}

#[tokio::test]
async fn test_real_limiter_5_launches_per_sec_for_10s() {
    let bus = TestBus::new().await.expect("Spawn test dbus");
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let script = tmp.path().join("quick_exit.sh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=Dummy\nExec={}\n",
        script.display()
    );
    fs::write(app_dir.join("dummy.desktop"), desktop).unwrap();

    let shared = Arc::new(AppsSharedState::new(vec![app_dir], "Kynex"));
    shared.reload().await;
    let server = AppsServer::new(Arc::clone(&shared));

    let _srv_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(APPS_PATH, server)
        .build()
        .await
        .expect("Server conn");

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client conn");

    let proxy = Apps1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(APPS_PATH)
        .build()
        .await
        .expect("Proxy build");

    // Test 5 launches/sec for 10 seconds = 50 launches total
    for sec in 0..10 {
        let sec_start = std::time::Instant::now();
        for i in 0..5 {
            let res = proxy.launch("dummy.desktop").await;
            assert!(
                res.is_ok(),
                "Launch {i} at second {sec} must succeed under rate limit, got error: {res:?}"
            );
        }
        // Wait until window resets (> 1.0s)
        let elapsed = sec_start.elapsed();
        if elapsed < Duration::from_millis(1050) {
            if let Some(remaining) = Duration::from_millis(1050).checked_sub(elapsed) {
                tokio::time::sleep(remaining).await;
            }
        }
    }

    // Wait up to 1 second for all children to be reaped
    let mut reaped = false;
    for _ in 0..20 {
        if shared.tracked_children_count() == 0 {
            reaped = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(reaped, "All children must be reaped");
}

#[tokio::test]
async fn test_daemon_wide_launch_rate_limiter() {
    let bus = TestBus::new().await.expect("Spawn test dbus");
    let tmp = tempdir().unwrap();
    let app_dir = tmp.path().join("applications");
    fs::create_dir_all(&app_dir).unwrap();

    let script = tmp.path().join("dummy.sh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=Dummy\nExec={}\n",
        script.display()
    );
    fs::write(app_dir.join("dummy.desktop"), desktop).unwrap();

    let shared = Arc::new(AppsSharedState::new(vec![app_dir], "Kynex"));
    shared.reload().await;
    let server = AppsServer::new(Arc::clone(&shared));

    let _srv_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(APPS_PATH, server)
        .build()
        .await
        .expect("Server conn");

    // Create 10 distinct D-Bus connections
    let mut proxies = Vec::new();
    for _ in 0..10 {
        let conn = zbus::connection::Builder::address(bus.address())
            .build()
            .await
            .expect("Client conn");
        let proxy = Apps1Proxy::builder(&conn)
            .destination(DAEMON_DESTINATION)
            .path(APPS_PATH)
            .build()
            .await
            .expect("Proxy build");
        proxies.push(proxy);
    }

    // Launch all 10 within 20 ms
    let start = std::time::Instant::now();
    let futures: Vec<_> = proxies.iter().map(|p| p.launch("dummy.desktop")).collect();
    let results = futures_util::future::join_all(futures).await;
    let duration = start.elapsed();
    assert!(
        duration < Duration::from_millis(500),
        "Launches took too long: {duration:?}"
    );

    let mut successes = 0;
    let mut rate_limited = 0;
    for res in results {
        match res {
            Ok(()) => successes += 1,
            Err(zbus::Error::FDO(fdo_err)) => match *fdo_err {
                zbus::fdo::Error::LimitsExceeded(_) => rate_limited += 1,
                other => panic!("Unexpected FDO error: {other:?}"),
            },
            Err(zbus::Error::MethodError(name, msg, _)) => {
                if name.as_str() == "org.freedesktop.DBus.Error.LimitsExceeded"
                    || msg.as_deref().unwrap_or("").contains("rate limit exceeded")
                {
                    rate_limited += 1;
                } else {
                    panic!("Unexpected error: {name} - {msg:?}");
                }
            }
            Err(e) => panic!("Unexpected error type: {e:?}"),
        }
    }

    assert_eq!(successes, 5, "Exactly 5 launches must succeed daemon-wide");
    assert_eq!(
        rate_limited, 5,
        "Exactly 5 launches must be rate-limited with LimitsExceeded"
    );
}
