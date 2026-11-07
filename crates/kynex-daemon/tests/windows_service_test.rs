//! Integration tests for `kynex-daemon` window tracking service with an isolated `dbus-daemon`.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use kynex_daemon::services::windows::{
    AllowAllSenderPolicy, KWinSenderPolicy, MockActionExecutor, SenderPolicy, WindowAction,
    WindowsFeedServer, WindowsServer, WindowsSharedState, ACTION_RATE_LIMIT_PER_SEC,
    FEED_RATE_LIMIT_PER_SEC,
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
    assert!(policy
        .verify_sender(":1.1", 99999, std::process::id(), Some(":1.1"))
        .is_err());
    // Current PID has current UID, but exe is the test binary, not kwin_wayland
    let caller_uid = rustix::process::getuid().as_raw();
    let host_process_id = std::process::id();
    assert!(policy
        .verify_sender(":1.1", caller_uid, host_process_id, Some(":1.1"))
        .is_err());

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

#[tokio::test]
async fn test_window_actions_happy_path() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let mock_executor = Arc::new(MockActionExecutor::new());
    shared.set_action_executor(mock_executor.clone()).await;

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

    // Seed a valid window
    feed_proxy
        .upsert("{1234-abcd}", "Test Editor", "editor", false, false)
        .await?;

    // Execute actions
    client_proxy.activate("{1234-abcd}").await?;
    client_proxy.minimize("{1234-abcd}").await?;
    client_proxy.restore("{1234-abcd}").await?;
    client_proxy.close("{1234-abcd}").await?;

    let calls = mock_executor.get_calls().await;
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls[0],
        ("{1234-abcd}".to_string(), WindowAction::Activate)
    );
    assert_eq!(
        calls[1],
        ("{1234-abcd}".to_string(), WindowAction::Minimize)
    );
    assert_eq!(calls[2], ("{1234-abcd}".to_string(), WindowAction::Restore));
    assert_eq!(calls[3], ("{1234-abcd}".to_string(), WindowAction::Close));

    Ok(())
}

#[tokio::test]
async fn test_window_actions_hostile_ids() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let mock_executor = Arc::new(MockActionExecutor::new());
    shared.set_action_executor(mock_executor.clone()).await;

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

    // Seed a legitimate window
    feed_proxy
        .upsert("{1234-abcd}", "Legit App", "app", false, false)
        .await?;

    let hostile_payloads = [
        r#""); evil();"#,
        r#""; evil(); //"#,
        "\"quote\"",
        "'single_quote'",
        "backslash\\path",
        "new\nline",
        "carriage\rreturn",
        "${INJECTION}",
        "$(rm -rf /)",
        "`touch /tmp/pwn`",
        "../../etc/passwd",
        "id with space",
        "",
        "null\0byte",
    ];

    for payload in hostile_payloads {
        let res_activate = client_proxy.activate(payload).await;
        assert!(
            res_activate.is_err(),
            "Hostile payload '{payload}' should have been rejected for activate"
        );

        let res_minimize = client_proxy.minimize(payload).await;
        assert!(
            res_minimize.is_err(),
            "Hostile payload '{payload}' should have been rejected for minimize"
        );

        let res_restore = client_proxy.restore(payload).await;
        assert!(
            res_restore.is_err(),
            "Hostile payload '{payload}' should have been rejected for restore"
        );

        let res_close = client_proxy.close(payload).await;
        assert!(
            res_close.is_err(),
            "Hostile payload '{payload}' should have been rejected for close"
        );
    }

    // Assert zero hostile calls reached the executor
    let calls = mock_executor.get_calls().await;
    assert_eq!(calls.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_window_actions_unknown_ids() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let mock_executor = Arc::new(MockActionExecutor::new());
    shared.set_action_executor(mock_executor.clone()).await;

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

    // Valid hex format, but not present in daemon state
    let res = client_proxy.activate("{9999-9999-9999}").await;
    let Err(err) = res else {
        unreachable!("Unknown window ID must be rejected");
    };

    let err_str = err.to_string();
    assert!(
        err_str.contains("does not exist in tracked state"),
        "Error message should explain ID does not exist: {err_str}"
    );

    let calls = mock_executor.get_calls().await;
    assert_eq!(calls.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_window_actions_kwin_gone() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let mock_executor = Arc::new(MockActionExecutor::new());
    mock_executor
        .set_fail_with(Some("KWin window manager service is unavailable".into()))
        .await;
    shared.set_action_executor(mock_executor.clone()).await;

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

    feed_proxy
        .upsert("{1234-abcd}", "Test Editor", "editor", false, false)
        .await?;

    let res = client_proxy.activate("{1234-abcd}").await;
    let Err(err) = res else {
        unreachable!("Call must fail when KWin is gone");
    };

    let err_str = err.to_string();
    assert!(
        err_str.contains("KWin window manager service is unavailable"),
        "Error should reflect KWin failure: {err_str}"
    );

    Ok(())
}

#[tokio::test]
async fn test_window_actions_rapid_1000_rate_limit() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let address = bus.address();

    let shared = Arc::new(WindowsSharedState::new(Arc::new(AllowAllSenderPolicy)));
    let mock_executor = Arc::new(MockActionExecutor::new());
    shared.set_action_executor(mock_executor.clone()).await;

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

    feed_proxy
        .upsert("{1234-abcd}", "Test Window", "app", false, false)
        .await?;

    let mut successful_calls = 0;
    let mut rate_limited_calls = 0;

    let calls = (0..1000).map(|_| client_proxy.activate("{1234-abcd}"));
    let results = futures_util::future::join_all(calls).await;

    for res in results {
        match res {
            Ok(()) => successful_calls += 1,
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("Rate limit exceeded") || err_msg.contains("LimitsExceeded") {
                    rate_limited_calls += 1;
                }
            }
        }
    }

    assert_eq!(
        successful_calls, ACTION_RATE_LIMIT_PER_SEC,
        "Exactly 20 actions should succeed within a 1-second burst"
    );
    assert_eq!(
        rate_limited_calls,
        1000 - ACTION_RATE_LIMIT_PER_SEC,
        "Remaining 980 actions must be cleanly rate-limited"
    );

    Ok(())
}

struct FakeKWinScripting {
    script_id_to_return: i32,
    unloaded: Arc<std::sync::atomic::AtomicBool>,
}

#[zbus::interface(name = "org.kde.kwin.Scripting")]
impl FakeKWinScripting {
    #[zbus(name = "loadScript")]
    fn load_script(&self, path: &str, name: &str) -> i32 {
        let _ = path;
        let _ = name;
        self.script_id_to_return
    }

    #[zbus(name = "unloadScript")]
    fn unload_script(&self, name: &str) -> bool {
        let _ = name;
        self.unloaded
            .store(true, std::sync::atomic::Ordering::Relaxed);
        true
    }
}

struct FakeKWinScript {
    ran: Arc<std::sync::atomic::AtomicBool>,
}

#[zbus::interface(name = "org.kde.kwin.Script")]
impl FakeKWinScript {
    #[zbus(name = "run")]
    fn run(&self) {
        self.ran.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[tokio::test]
async fn test_kwin_script_action_executor_lifecycle() -> Result<(), Box<dyn Error>> {
    use kynex_daemon::services::windows::{KWinActionExecutor, KWinScriptActionExecutor};

    let bus = TestBus::new().await?;
    let address = bus.address();

    let ran_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let unloaded_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let scripting_server = FakeKWinScripting {
        script_id_to_return: 0,
        unloaded: Arc::clone(&unloaded_flag),
    };
    let script_server = FakeKWinScript {
        ran: Arc::clone(&ran_flag),
    };

    // Serve fake KWin on the bus
    let _kwin_conn = zbus::connection::Builder::address(address)
        .name("org.kde.KWin")
        .serve_at("/Scripting", scripting_server)
        .serve_at("/Scripting/Script0", script_server)
        .build()
        .await?;

    let client_conn = connect_to_bus(address).await?;
    let executor = KWinScriptActionExecutor::new(client_conn);

    // 1. Success execution
    let res = executor
        .execute_action("{1234-abcd}", WindowAction::Activate)
        .await;
    assert!(res.is_ok(), "Execution should succeed on fake KWin");
    assert!(ran_flag.load(std::sync::atomic::Ordering::Relaxed));
    assert!(unloaded_flag.load(std::sync::atomic::Ordering::Relaxed));

    // 2. KWin returns error script ID (-1)
    let bad_scripting_server = FakeKWinScripting {
        script_id_to_return: -1,
        unloaded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    let bus2 = TestBus::new().await?;
    let _kwin_conn2 = zbus::connection::Builder::address(bus2.address())
        .name("org.kde.KWin")
        .serve_at("/Scripting", bad_scripting_server)
        .build()
        .await?;
    let client_conn2 = connect_to_bus(bus2.address()).await?;
    let executor2 = KWinScriptActionExecutor::new(client_conn2);
    let res_err_id = executor2
        .execute_action("{1234-abcd}", WindowAction::Close)
        .await;
    let Err(err_id) = res_err_id else {
        unreachable!("Should return error on ID -1");
    };
    assert!(err_id.contains("returned error script ID"));

    // 3. KWin not running
    let bus3 = TestBus::new().await?;
    let client_conn3 = connect_to_bus(bus3.address()).await?;
    let executor3 = KWinScriptActionExecutor::new(client_conn3);
    let res_no_kwin = executor3
        .execute_action("{1234-abcd}", WindowAction::Minimize)
        .await;
    let Err(err_kwin) = res_no_kwin else {
        unreachable!("Should return error when KWin is not running");
    };
    assert!(err_kwin.contains("KWin window manager service is unavailable"));

    Ok(())
}
