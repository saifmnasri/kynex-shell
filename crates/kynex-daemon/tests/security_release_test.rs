#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::print_stdout,
    clippy::print_stderr
)]
//! R3.6 Security verification test for test hooks.
//!
//! Proves that test environment hooks are strictly gated and that non-KWin callers
//! are rejected with `AccessDenied` even when `KYNEX_ALLOW_TEST_FEEDER=1` is present.

use kynex_ipc::windows::WindowsFeed1Proxy;
use kynex_ipc::DAEMON_DESTINATION;
use std::error::Error;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

struct TestBus {
    child: tokio::process::Child,
    address: String,
}

impl TestBus {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let mut cmd = Command::new("dbus-daemon");
        cmd.arg("--session")
            .arg("--print-address=1")
            .arg("--nofork")
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

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

#[tokio::test]
async fn test_release_daemon_ignores_test_feeder_env_and_rejects_unauthorized(
) -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    // Locate or build a clean release kynex-daemon binary (built strictly without test-hooks)
    let daemon_bin = {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let target_dir = root.join("target/release-clean");
        let rel_bin = target_dir.join("release/kynex-daemon");
        if !rel_bin.exists() {
            let status = std::process::Command::new("cargo")
                .args([
                    "build",
                    "--release",
                    "-p",
                    "kynex-daemon",
                    "--no-default-features",
                    "--target-dir",
                ])
                .arg(&target_dir)
                .status()?;
            assert!(
                status.success(),
                "Failed to build clean release kynex-daemon"
            );
        }
        rel_bin
    };

    // Spawn kynex-daemon with test-hook variables set
    let mut cmd = Command::new(daemon_bin);
    cmd.env("DBUS_SESSION_BUS_ADDRESS", address)
        .env("KYNEX_ALLOW_TEST_FEEDER", "1")
        .env("KYNEX_TEST_PACTL_PATH", "/nonexistent/test/pactl")
        .env("KYNEX_UPOWER_BUS", "session")
        .env("KYNEX_NM_BUS", "session")
        .env("RUST_LOG", "debug")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut daemon_child = cmd.spawn()?;

    // Wait for daemon to register on bus
    let client_conn = zbus::connection::Builder::address(address).build().await?;
    let dbus_proxy = zbus::fdo::DBusProxy::new(&client_conn).await?;

    let mut registered = false;
    for _ in 0..60 {
        if dbus_proxy
            .name_has_owner(DAEMON_DESTINATION.try_into()?)
            .await
            .unwrap_or(false)
        {
            registered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(
        registered,
        "kynex-daemon failed to claim '{DAEMON_DESTINATION}' on test bus"
    );

    // Attempt to invoke private feeder interface from this test binary (not /usr/bin/kwin_wayland)
    let feeder_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    let reset_result = feeder_proxy.reset().await;

    // Must be rejected with AccessDenied because KWinSenderPolicy is enforced!
    assert!(
        reset_result.is_err(),
        "Expected feeder call from non-KWin caller to fail, but it succeeded!"
    );

    let err_str = reset_result.unwrap_err().to_string();
    assert!(
        err_str.contains("AccessDenied") || err_str.contains("feeder auth"),
        "Expected AccessDenied error, got: {err_str}"
    );

    // Stop daemon cleanly
    let _ = daemon_child.start_kill();

    Ok(())
}
