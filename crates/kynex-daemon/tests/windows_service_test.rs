//! Integration tests for `kynex-daemon` window tracking service with an isolated `dbus-daemon`.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use kynex_daemon::services::windows::{
    AllowAllSenderPolicy, KWinSenderPolicy, SenderPolicy, WindowsFeedServer, WindowsServer,
    WindowsSharedState, FEED_RATE_LIMIT_PER_SEC,
};
use kynex_daemon::window_state::MAX_WINDOWS;
use kynex_ipc::windows::{Windows1Proxy, WindowsFeed1Proxy, WINDOWS_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use tokio::io::AsyncBufReadExt;
use zbus::object_server::SignalEmitter;
use zbus::Connection;

/// Helper managing a private, isolated `dbus-daemon` instance for integration testing.
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

/// Helper to connect a zbus connection to the private test bus.
async fn connect_to_bus(address: &str) -> Result<Connection, Box<dyn Error>> {
    let conn = zbus::connection::Builder::address(address).build().await?;
    Ok(conn)
}

#[tokio::test]
async fn test_fake_kwin_feeder_lifecycle() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = WindowsServer::new(Arc::clone(&shared));
    let feed_server = WindowsFeedServer::new(Arc::clone(&shared));

    let server_conn = zbus::connection::Builder::address(address)
        .name(DAEMON_DESTINATION)
        .serve_at(WINDOWS_PATH, server.clone())
        .serve_at(WINDOWS_PATH, feed_server.clone())
        .build()
        .await?;

    let emitter = SignalEmitter::new(&server_conn, WINDOWS_PATH)?;
    feed_server.set_emitter(emitter).await;

    let client_conn = connect_to_bus(address).await?;
    let client_proxy = Windows1Proxy::new(&client_conn).await?;
    let feed_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    // Initial state: empty
    let list = client_proxy.list().await?;
    assert_eq!(list.len(), 0);

    // 1. Upsert a new window
    feed_proxy
        .upsert(
            "{c7c4f456-4c47}",
            "Terminal",
            "org.kde.konsole",
            false,
            true,
        )
        .await?;

    let list_after_add = client_proxy.list().await?;
    assert_eq!(list_after_add.len(), 1);
    assert_eq!(list_after_add[0].id, "{c7c4f456-4c47}");
    assert_eq!(list_after_add[0].title, "Terminal");
    assert_eq!(list_after_add[0].app_id, "org.kde.konsole");
    assert!(!list_after_add[0].minimized);
    assert!(list_after_add[0].active);

    // 2. Update existing window (change title & minimized)
    feed_proxy
        .upsert(
            "{c7c4f456-4c47}",
            "Terminal - vim",
            "org.kde.konsole",
            true,
            true,
        )
        .await?;

    let list_after_update = client_proxy.list().await?;
    assert_eq!(list_after_update.len(), 1);
    assert_eq!(list_after_update[0].title, "Terminal - vim");
    assert!(list_after_update[0].minimized);

    // 3. Add second window as active -> first must become inactive
    feed_proxy
        .upsert("{dead-beef-cafe}", "Browser", "firefox", false, true)
        .await?;

    let list_two = client_proxy.list().await?;
    assert_eq!(list_two.len(), 2);
    assert!(!list_two[0].active);
    assert!(list_two[1].active);

    // 4. SetActive
    feed_proxy.set_active("{c7c4f456-4c47}").await?;
    let list_switched = client_proxy.list().await?;
    assert!(list_switched[0].active);
    assert!(!list_switched[1].active);

    // 5. Remove second window
    feed_proxy.remove("{dead-beef-cafe}").await?;
    let list_after_remove = client_proxy.list().await?;
    assert_eq!(list_after_remove.len(), 1);
    assert_eq!(list_after_remove[0].id, "{c7c4f456-4c47}");

    // 6. Reset
    feed_proxy.reset().await?;
    let list_after_reset = client_proxy.list().await?;
    assert_eq!(list_after_reset.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_production_policy_rejects_spoofed_caller() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    // Setup server with production policy (requires caller to be kwin_wayland)
    let shared = Arc::new(WindowsSharedState::new(Arc::new(KWinSenderPolicy::new())));
    let server = WindowsServer::new(Arc::clone(&shared));
    let feed_server = WindowsFeedServer::new(Arc::clone(&shared));

    let _server_conn = zbus::connection::Builder::address(address)
        .name(DAEMON_DESTINATION)
        .serve_at(WINDOWS_PATH, server)
        .serve_at(WINDOWS_PATH, feed_server)
        .build()
        .await?;

    // Attempt to invoke feed methods from the test process (which is NOT kwin_wayland)
    let client_conn = connect_to_bus(address).await?;
    let feed_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    let result = feed_proxy
        .upsert("win-1", "Spoofed Title", "spoof.app", false, false)
        .await;

    // Production policy must reject with an error (AccessDenied)
    assert!(result.is_err());
    if let Err(err) = result {
        let err_str = err.to_string();
        assert!(
            err_str.contains("AccessDenied") || err_str.contains("not kwin_wayland"),
            "Unexpected error: {err_str}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_hostile_titles_and_input_sanitization() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = WindowsServer::new(Arc::clone(&shared));
    let feed_server = WindowsFeedServer::new(Arc::clone(&shared));

    let _server_conn = zbus::connection::Builder::address(address)
        .name(DAEMON_DESTINATION)
        .serve_at(WINDOWS_PATH, server)
        .serve_at(WINDOWS_PATH, feed_server)
        .build()
        .await?;

    let client_conn = connect_to_bus(address).await?;
    let client_proxy = Windows1Proxy::new(&client_conn).await?;
    let feed_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    // 1. Hostile title with control characters & bidi override
    let hostile_title = "Normal\x07Control\x1bChars\r\n\t\u{202E}Spoofed\u{2066}End";
    feed_proxy
        .upsert("1111", hostile_title, "safe.app", false, false)
        .await?;

    let list = client_proxy.list().await?;
    assert_eq!(list[0].title, "NormalControlCharsSpoofedEnd");

    // 2. Oversized title > 256 bytes with multi-byte unicode
    let mut large_title = "a".repeat(254);
    large_title.push('🦀'); // 4-byte char crossing boundary
    feed_proxy
        .upsert("2222", &large_title, "safe.app", false, false)
        .await?;

    let list = client_proxy.list().await?;
    let win2 = list.iter().find(|w| w.id == "2222");
    assert!(win2.is_some());
    if let Some(w) = win2 {
        assert_eq!(w.title.len(), 254);
        assert!(w.title.chars().all(|c| c == 'a'));
    }

    // 3. Malformed appId replaced by ""
    feed_proxy
        .upsert("3333", "App", "invalid/path app!", false, false)
        .await?;

    let list = client_proxy.list().await?;
    let win3 = list.iter().find(|w| w.id == "3333");
    assert!(win3.is_some());
    if let Some(w) = win3 {
        assert_eq!(w.app_id, "");
    }

    // 4. Invalid window ID rejected with error
    let invalid_id_result = feed_proxy
        .upsert("invalid id with spaces!", "Title", "app", false, false)
        .await;
    assert!(invalid_id_result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_window_storm_capacity_bound() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = WindowsServer::new(Arc::clone(&shared));
    let feed_server = WindowsFeedServer::new(Arc::clone(&shared));

    let _server_conn = zbus::connection::Builder::address(address)
        .name(DAEMON_DESTINATION)
        .serve_at(WINDOWS_PATH, server)
        .serve_at(WINDOWS_PATH, feed_server)
        .build()
        .await?;

    let client_conn = connect_to_bus(address).await?;
    let client_proxy = Windows1Proxy::new(&client_conn).await?;
    let feed_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    // Feed windows rapidly through direct state machine to test 1,000-window storm
    // Note: D-Bus feed is rate limited to 200/s, but internal state handles 1,000-window storms
    let mut direct_state = shared.state.write().await;
    for i in 0..1000 {
        let id = format!("{i:04x}");
        let _ = direct_state.upsert(&id, "Storm Window", "storm.app", false, false);
    }
    assert_eq!(direct_state.count(), MAX_WINDOWS);
    drop(direct_state);

    let list = client_proxy.list().await?;
    assert_eq!(list.len(), MAX_WINDOWS);

    // Any further insertion via D-Bus feed fails with LimitsExceeded
    let overflow = feed_proxy
        .upsert("ffff", "Overflow", "storm.app", false, false)
        .await;
    assert!(overflow.is_err());

    Ok(())
}

#[tokio::test]
async fn test_rate_limiting_enforcement() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = WindowsServer::new(Arc::clone(&shared));
    let feed_server = WindowsFeedServer::new(Arc::clone(&shared));

    let _server_conn = zbus::connection::Builder::address(address)
        .name(DAEMON_DESTINATION)
        .serve_at(WINDOWS_PATH, server)
        .serve_at(WINDOWS_PATH, feed_server)
        .build()
        .await?;

    let client_conn = connect_to_bus(address).await?;
    let feed_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    let mut success_count = 0;
    let mut rate_limited = false;

    for i in 0..(FEED_RATE_LIMIT_PER_SEC + 50) {
        let id = format!("{i:04x}");
        let res = feed_proxy.upsert(&id, "Title", "app", false, false).await;
        if res.is_ok() {
            success_count += 1;
        } else {
            rate_limited = true;
            break;
        }
    }

    assert!(rate_limited, "Expected rate limit to kick in");
    assert_eq!(success_count, FEED_RATE_LIMIT_PER_SEC);
    assert!(
        shared
            .dropped_calls
            .load(std::sync::atomic::Ordering::Relaxed)
            > 0
    );

    Ok(())
}

#[tokio::test]
async fn test_sender_policy_and_clear_coverage() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    // 1. Test KWinSenderPolicy unit checks directly
    let policy = KWinSenderPolicy::default();
    // Non-matching UID
    assert!(policy.verify_sender(99999, std::process::id()).is_err());
    // Current PID has current UID, but exe is the test binary, not kwin_wayland
    let caller_uid = rustix::process::getuid().as_raw();
    let host_process_id = std::process::id();
    assert!(policy.verify_sender(caller_uid, host_process_id).is_err());

    // 2. Server snapshot & clear
    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = WindowsServer::new(Arc::clone(&shared));
    let feed_server = WindowsFeedServer::new(Arc::clone(&shared));

    let server_conn = zbus::connection::Builder::address(address)
        .name(DAEMON_DESTINATION)
        .serve_at(WINDOWS_PATH, server.clone())
        .serve_at(WINDOWS_PATH, feed_server.clone())
        .build()
        .await?;

    let emitter = SignalEmitter::new(&server_conn, WINDOWS_PATH)?;
    feed_server.set_emitter(emitter.clone()).await;

    let client_conn = connect_to_bus(address).await?;
    let client_proxy = Windows1Proxy::new(&client_conn).await?;
    let feed_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    // Upsert window 1 and window 2 (active)
    feed_proxy
        .upsert("1111", "One", "app.one", false, false)
        .await?;
    feed_proxy
        .upsert("2222", "Two", "app.two", false, true)
        .await?;

    // Unchanged upsert
    feed_proxy
        .upsert("1111", "One", "app.one", false, false)
        .await?;

    // Check server snapshot
    let snap = server.snapshot().await;
    assert_eq!(snap.len(), 2);

    // Remove active window -> must trigger active_changed("")
    feed_proxy.remove("2222").await?;
    assert_eq!(client_proxy.list().await?.len(), 1);

    // Clear server
    server.clear(&emitter).await;
    assert_eq!(client_proxy.list().await?.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_kwin_watcher_disappearance() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = WindowsServer::new(Arc::clone(&shared));
    let feed_server = WindowsFeedServer::new(Arc::clone(&shared));

    let server_conn = zbus::connection::Builder::address(address)
        .name(DAEMON_DESTINATION)
        .serve_at(WINDOWS_PATH, server.clone())
        .serve_at(WINDOWS_PATH, feed_server.clone())
        .build()
        .await?;

    let emitter = SignalEmitter::new(&server_conn, WINDOWS_PATH)?;
    feed_server.set_emitter(emitter.clone()).await;

    // Spawn watcher
    tokio::spawn(kynex_daemon::services::windows::run_kwin_watcher(
        server_conn.clone(),
        server.clone(),
        emitter,
    ));

    // Claim org.kde.KWin with a temporary connection then drop it
    let kwin_conn = zbus::connection::Builder::address(address)
        .name("org.kde.KWin")
        .build()
        .await?;

    let client_conn = connect_to_bus(address).await?;
    let client_proxy = Windows1Proxy::new(&client_conn).await?;
    let feed_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    feed_proxy
        .upsert("aaaa", "KWin Window", "app", false, true)
        .await?;
    assert_eq!(client_proxy.list().await?.len(), 1);

    // Drop KWin connection -> name is lost
    drop(kwin_conn);

    // Give watcher time to receive signal and clear
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        if client_proxy.list().await?.is_empty() {
            break;
        }
    }

    assert_eq!(client_proxy.list().await?.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_kwin_restart_reset() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let server = WindowsServer::new(Arc::clone(&shared));
    let feed_server = WindowsFeedServer::new(Arc::clone(&shared));

    let _server_conn = zbus::connection::Builder::address(address)
        .name(DAEMON_DESTINATION)
        .serve_at(WINDOWS_PATH, server)
        .serve_at(WINDOWS_PATH, feed_server)
        .build()
        .await?;

    let client_conn = connect_to_bus(address).await?;
    let client_proxy = Windows1Proxy::new(&client_conn).await?;
    let feed_proxy = WindowsFeed1Proxy::new(&client_conn).await?;

    // Add multiple windows
    feed_proxy
        .upsert("1111", "One", "app", false, false)
        .await?;
    feed_proxy.upsert("2222", "Two", "app", false, true).await?;
    assert_eq!(client_proxy.list().await?.len(), 2);

    // KWin restarts and sends Reset()
    feed_proxy.reset().await?;
    assert_eq!(client_proxy.list().await?.len(), 0);

    Ok(())
}
