#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::items_after_statements
)]
//! Introspection XML validation test suite.
//!
//! Validates:
//! - All exported leaf object paths introspect cleanly with `busctl introspect` (exits 0, empty stderr).
//! - No `<signal>` contains an `<arg>` with `direction="in"` across any exported object path.
//! - Upstream systemd `busctl-introspect.c` leak documentation and verification.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

struct TestBus {
    child: tokio::process::Child,
    address: String,
}

impl TestBus {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut cmd = Command::new("dbus-daemon");
        cmd.arg("--session")
            .arg("--print-address=1")
            .arg("--nofork")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().expect("dbus stdout");
        let mut reader = BufReader::new(stdout);
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

use kynex_daemon::services::apps::{AppsServer, AppsSharedState};
use kynex_daemon::services::audio::{AudioServer, PactlAudioBackend};
use kynex_daemon::services::network::NetworkServer;
use kynex_daemon::services::notifications::{
    FdoNotificationsServer, NotificationsServer, NotificationsSharedState,
};
use kynex_daemon::services::pins::PinsServer;
use kynex_daemon::services::power::PowerServer;
use kynex_daemon::services::shell::{ShellFeedServer, ShellServer, ShellSharedState};
use kynex_daemon::services::tray::{
    FdoStatusNotifierWatcherServer, StatusNotifierWatcherServer, TrayServer, TraySharedState,
};
use kynex_daemon::services::windows::{
    AllowAllSenderPolicy, SenderPolicy, WindowsFeedServer, WindowsServer, WindowsSharedState,
};
use kynex_daemon::DaemonServer;
use kynex_ipc::{
    apps::APPS_PATH, audio::AUDIO_PATH, notifications::FDO_NOTIFICATIONS_PATH,
    notifications::NOTIFICATIONS_PATH, pins::PINS_PATH, shell::SHELL_PATH,
    tray::STATUS_NOTIFIER_WATCHER_PATH, tray::TRAY_PATH, DAEMON_PATH, NETWORK_PATH, POWER_PATH,
    WINDOWS_PATH,
};

#[tokio::test]
async fn test_all_leaf_object_paths_introspect_cleanly() {
    let running = Arc::new(AtomicBool::new(true));
    let core_server = DaemonServer::new(running);
    let power_server = PowerServer::new();
    let network_server = NetworkServer::new();

    let sender_policy: Arc<dyn SenderPolicy> = Arc::new(AllowAllSenderPolicy);
    let windows_shared = Arc::new(WindowsSharedState::new(Arc::clone(&sender_policy)));
    let windows_server = WindowsServer::new(Arc::clone(&windows_shared));
    let windows_feed_server = WindowsFeedServer::new(Arc::clone(&windows_shared));

    let shell_shared = Arc::new(ShellSharedState::new(sender_policy));
    let shell_server = ShellServer::new(Arc::clone(&shell_shared));
    let shell_feed_server = ShellFeedServer::new(Arc::clone(&shell_shared));

    let app_dirs = kynex_apps_index::AppsIndex::standard_app_dirs();
    let apps_shared = Arc::new(AppsSharedState::new(app_dirs, "Kynex"));
    let apps_server = AppsServer::new(Arc::clone(&apps_shared));

    let pins_server = PinsServer::new_default();

    let notif_shared = Arc::new(NotificationsSharedState::new(Some(Arc::clone(
        &apps_shared,
    ))));
    let notif_server = NotificationsServer::new(Arc::clone(&notif_shared));
    let fdo_notif_server = FdoNotificationsServer::new(Arc::clone(&notif_shared));

    let tray_shared = Arc::new(TraySharedState::new());
    let kde_watcher_server = StatusNotifierWatcherServer::new(Arc::clone(&tray_shared));
    let fdo_watcher_server = FdoStatusNotifierWatcherServer::new(Arc::clone(&tray_shared));
    let tray_server = TrayServer::new(Arc::clone(&tray_shared));

    let audio_backend = Arc::new(PactlAudioBackend::new());
    let audio_server = AudioServer::new(audio_backend);

    let test_bus_name = "org.kynexos.Daemon.IntrospectValidationTest";

    let test_bus = TestBus::new()
        .await
        .expect("Failed to spawn private dbus-daemon for introspection test");
    let bus_addr = test_bus.address();
    let addr_arg = format!("--address={bus_addr}");

    let conn = zbus::connection::Builder::address(bus_addr)
        .name(test_bus_name)
        .serve_at(DAEMON_PATH, core_server)
        .serve_at(POWER_PATH, power_server)
        .serve_at(NETWORK_PATH, network_server)
        .serve_at(WINDOWS_PATH, windows_server)
        .serve_at(WINDOWS_PATH, windows_feed_server)
        .serve_at(SHELL_PATH, shell_server)
        .serve_at(SHELL_PATH, shell_feed_server)
        .serve_at(APPS_PATH, apps_server)
        .serve_at(PINS_PATH, pins_server)
        .serve_at(NOTIFICATIONS_PATH, notif_server)
        .serve_at(FDO_NOTIFICATIONS_PATH, fdo_notif_server)
        .serve_at(STATUS_NOTIFIER_WATCHER_PATH, kde_watcher_server)
        .serve_at(STATUS_NOTIFIER_WATCHER_PATH, fdo_watcher_server)
        .serve_at(TRAY_PATH, tray_server)
        .serve_at(AUDIO_PATH, audio_server)
        .build()
        .await
        .unwrap();

    kynex_daemon::introspection::setup_clean_introspection(&conn)
        .await
        .unwrap();

    // 0. Verify `busctl tree` produces exit code 0 and completely EMPTY stderr
    let tree_output = tokio::process::Command::new("busctl")
        .args([&addr_arg, "tree", test_bus_name])
        .output()
        .await
        .expect("Failed to run busctl tree");
    let tree_stderr = String::from_utf8_lossy(&tree_output.stderr);
    assert!(
        tree_output.status.success(),
        "busctl tree failed with status {}: {tree_stderr}",
        tree_output.status
    );
    assert!(
        tree_stderr.trim().is_empty(),
        "busctl tree emitted unexpected stderr:\n{tree_stderr}"
    );

    let all_paths = [
        "/",
        "/org",
        "/org/freedesktop",
        "/org/kynexos",
        "/org/kynexos/Daemon",
        WINDOWS_PATH,
        APPS_PATH,
        SHELL_PATH,
        PINS_PATH,
        AUDIO_PATH,
        NOTIFICATIONS_PATH,
        TRAY_PATH,
        STATUS_NOTIFIER_WATCHER_PATH,
        FDO_NOTIFICATIONS_PATH,
    ];

    for path in all_paths {
        // 1. Verify `busctl introspect` runs with zero warnings and empty stderr
        let output = tokio::process::Command::new("busctl")
            .args([&addr_arg, "introspect", test_bus_name, path])
            .output()
            .await
            .unwrap_or_else(|e| panic!("Failed to run busctl introspect on {path}: {e}"));

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "busctl introspect on {path} failed: {stderr}"
        );
        assert!(
            !stderr.contains("Unexpected signal <arg> direction value 'in'"),
            "Leaf path {path} emitted unexpected signal arg direction warning: {stderr}"
        );
        assert!(
            stderr.trim().is_empty(),
            "Leaf path {path} emitted non-empty stderr: {stderr}"
        );

        // 2. Fetch raw XML and parse with XML DOM to assert zero <signal> args specify direction="in"
        let xml_output = tokio::process::Command::new("busctl")
            .args([
                &addr_arg,
                "introspect",
                test_bus_name,
                path,
                "--xml-interface",
            ])
            .output()
            .await
            .unwrap_or_else(|e| {
                panic!("Failed to run busctl introspect --xml-interface on {path}: {e}")
            });

        assert!(xml_output.status.success());
        let xml_str = String::from_utf8_lossy(&xml_output.stdout);

        // Run python XML AST verification
        let mut py_check = tokio::process::Command::new("python3")
            .args([
                "-c",
                r#"
import sys, xml.etree.ElementTree as ET
xml_data = sys.stdin.read()
root = ET.fromstring(xml_data)
violations = []
for sig in root.iter('signal'):
    sig_name = sig.attrib.get('name', '<unnamed>')
    for arg in sig.findall('arg'):
        direction = arg.attrib.get('direction')
        if direction == 'in':
            violations.append(f"signal {sig_name} has invalid arg direction='in'")
if violations:
    print("; ".join(violations))
    sys.exit(1)
"#,
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to spawn python3 for XML check");

        use tokio::io::AsyncWriteExt;
        let mut py_stdin = py_check.stdin.take().expect("Failed to open stdin");
        py_stdin
            .write_all(xml_str.as_bytes())
            .await
            .expect("Failed to write to stdin");
        drop(py_stdin);

        let py_output = py_check
            .wait_with_output()
            .await
            .expect("Failed to wait for python3");
        assert!(
            py_output.status.success(),
            "XML check failed for path {path}: {}",
            String::from_utf8_lossy(&py_output.stdout)
        );
    }

    // 3. Verify busctl tree exits 0 with completely empty stderr
    let tree_output = tokio::process::Command::new("busctl")
        .args([&addr_arg, "tree", test_bus_name])
        .output()
        .await
        .expect("Failed to run busctl tree");
    assert!(tree_output.status.success());
    let tree_stderr = String::from_utf8_lossy(&tree_output.stderr);
    assert!(
        tree_stderr.trim().is_empty(),
        "busctl tree emitted warnings on stderr: {tree_stderr}"
    );
}
