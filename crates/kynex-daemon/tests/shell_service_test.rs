#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]
//! Integration tests for the daemon Shell service (`org.kynexos.Daemon.Shell1` and `ShellFeed1`).

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use kynex_daemon::services::shell::{ShellFeedServer, ShellServer, ShellSharedState};
use kynex_daemon::services::windows::{AllowAllSenderPolicy, KWinSenderPolicy};
use kynex_ipc::shell::{Shell1Proxy, ShellFeed1Proxy, SHELL_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use zbus::object_server::SignalEmitter;

mod test_bus {
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;
    use tempfile::{tempdir, TempDir};

    pub struct TestBus {
        _dir: TempDir,
        address: String,
        process: Child,
    }

    impl TestBus {
        pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let dir = tempdir()?;
            let socket_path = dir.path().join("dbus.sock");
            let address = format!("unix:path={}", socket_path.display());
            let config_path = dir.path().join("session.conf");

            let config_content = format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>{address}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>"#
            );
            std::fs::write(&config_path, config_content)?;

            let child = Command::new("dbus-daemon")
                .arg(format!("--config-file={}", config_path.display()))
                .arg("--nofork")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;

            for _ in 0..50 {
                if socket_path.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            Ok(Self {
                _dir: dir,
                address,
                process: child,
            })
        }

        pub fn address(&self) -> &str {
            &self.address
        }
    }

    impl Drop for TestBus {
        fn drop(&mut self) {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

use test_bus::TestBus;

#[tokio::test]
async fn test_shell_shortcut_authorized_triggers_signal() {
    let bus = TestBus::new().await.expect("Spawn test bus");

    let shared = Arc::new(ShellSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = ShellServer::new(Arc::clone(&shared));
    let feed_server = ShellFeedServer::new(Arc::clone(&shared));

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(SHELL_PATH, server)
        .serve_at(SHELL_PATH, feed_server.clone())
        .build()
        .await
        .expect("Daemon conn build");

    let emitter = SignalEmitter::new(&daemon_conn, SHELL_PATH).expect("Emitter build");
    feed_server.set_emitter(emitter).await;

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client conn build");

    let shell_proxy = Shell1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(SHELL_PATH)
        .build()
        .await
        .expect("Shell1Proxy build");

    let mut signal_stream = shell_proxy
        .receive_launcher_toggle_requested()
        .await
        .expect("Receive signal stream");

    let feed_proxy = ShellFeed1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(SHELL_PATH)
        .build()
        .await
        .expect("ShellFeed1Proxy build");

    // Call LauncherShortcut
    feed_proxy
        .launcher_shortcut()
        .await
        .expect("LauncherShortcut must succeed");

    // Await signal
    let received = tokio::time::timeout(Duration::from_secs(2), signal_stream.next()).await;
    assert!(
        received.is_ok() && received.unwrap().is_some(),
        "LauncherToggleRequested signal must be received after LauncherShortcut call"
    );
}

#[tokio::test]
async fn test_shell_shortcut_unauthorized_caller_rejected() {
    let bus = TestBus::new().await.expect("Spawn test bus");

    // Standard policy requiring /usr/bin/kwin_wayland
    let shared = Arc::new(ShellSharedState::new(Arc::new(KWinSenderPolicy::new())));
    let server = ShellServer::new(Arc::clone(&shared));
    let feed_server = ShellFeedServer::new(Arc::clone(&shared));

    let _daemon_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(SHELL_PATH, server)
        .serve_at(SHELL_PATH, feed_server)
        .build()
        .await
        .expect("Daemon conn build");

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client conn build");

    let feed_proxy = ShellFeed1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(SHELL_PATH)
        .build()
        .await
        .expect("ShellFeed1Proxy build");

    // Calling from test runner (not kwin_wayland) must be rejected
    let res = feed_proxy.launcher_shortcut().await;
    assert!(res.is_err(), "Non-KWin caller must be rejected");

    let err = res.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("AccessDenied") || err_str.contains("Access denied"),
        "Expected AccessDenied error, got: {err_str}"
    );

    assert_eq!(
        shared.dropped_calls(),
        1,
        "Dropped calls counter must be incremented"
    );
}

#[tokio::test]
async fn test_shell_shortcut_rate_limiting() {
    let bus = TestBus::new().await.expect("Spawn test bus");

    let shared = Arc::new(ShellSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = ShellServer::new(Arc::clone(&shared));
    let feed_server = ShellFeedServer::new(Arc::clone(&shared));

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(SHELL_PATH, server)
        .serve_at(SHELL_PATH, feed_server.clone())
        .build()
        .await
        .expect("Daemon conn build");

    let emitter = SignalEmitter::new(&daemon_conn, SHELL_PATH).expect("Emitter build");
    feed_server.set_emitter(emitter).await;

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client conn build");

    let feed_proxy = ShellFeed1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(SHELL_PATH)
        .build()
        .await
        .expect("ShellFeed1Proxy build");

    let mut hit_rate_limit = false;
    for _ in 0..30 {
        if feed_proxy.launcher_shortcut().await.is_err() {
            hit_rate_limit = true;
            break;
        }
    }
    assert!(hit_rate_limit, "Rate limiter must reject excess calls");
}

#[tokio::test]
async fn test_shell_feed_without_emitter() {
    let bus = TestBus::new().await.expect("Spawn test bus");

    let shared = Arc::new(ShellSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = ShellServer::new(Arc::clone(&shared));
    assert!(Arc::ptr_eq(server.shared(), &shared));

    let feed_server = ShellFeedServer::new(Arc::clone(&shared));

    let _daemon_conn = zbus::connection::Builder::address(bus.address())
        .name(DAEMON_DESTINATION)
        .serve_at(SHELL_PATH, server)
        .serve_at(SHELL_PATH, feed_server)
        .build()
        .await
        .expect("Daemon conn build");

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Client conn build");

    let feed_proxy = ShellFeed1Proxy::builder(&client_conn)
        .destination(DAEMON_DESTINATION)
        .path(SHELL_PATH)
        .build()
        .await
        .expect("ShellFeed1Proxy build");

    let res = feed_proxy.launcher_shortcut().await;
    assert!(
        res.is_ok(),
        "Shortcut call without emitter must still succeed without panic"
    );
}

#[tokio::test]
async fn test_shell_kwin_watcher_owner_lifecycle() {
    let bus = TestBus::new().await.expect("Spawn test bus");

    let shared = Arc::new(ShellSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let watcher_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("Watcher conn");

    let watcher_shared = Arc::clone(&shared);
    let watcher_handle = tokio::spawn(async move {
        kynex_daemon::services::shell::run_shell_kwin_watcher(watcher_conn, watcher_shared).await;
    });

    // KWin connection appears and requests name org.kde.KWin
    let kwin_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await
        .expect("KWin conn");

    let kwin_unique_name = kwin_conn.unique_name().map(ToString::to_string);

    kwin_conn
        .request_name("org.kde.KWin")
        .await
        .expect("Request org.kde.KWin name");

    // Watcher should pick up KWin unique name
    let mut tracked = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(ref owner) = *shared.tracked_kwin_owner.read().await {
            if Some(owner.clone()) == kwin_unique_name {
                tracked = true;
                break;
            }
        }
    }
    assert!(tracked, "Watcher must track active org.kde.KWin owner");

    // KWin drops the name
    kwin_conn
        .release_name("org.kde.KWin")
        .await
        .expect("Release name");

    let mut cleared = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if shared.tracked_kwin_owner.read().await.is_none() {
            cleared = true;
            break;
        }
    }
    assert!(cleared, "Watcher must clear tracked owner when released");

    watcher_handle.abort();
}
