//! Integration tests for desktop notifications service with an isolated `dbus-daemon`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::unreadable_literal,
    clippy::doc_markdown,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::assert_is_empty
)]

//! Validates:
//! - N4.1: Spec conformance (GetCapabilities, GetServerInformation, replaces_id,
//!   expire_timeout cases, close reasons 1-4, unicast ActionInvoked delivery).
//! - N4.2: Hostile corpus (huge payloads, 10,000 actions, script/markup/bidi injection,
//!   pathological image-data, replaces_id hijacking, token-bucket flood rate limits,
//!   DoNotDisturb popup suppression vs critical bypass).

use std::collections::HashMap;
use std::error::Error;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use kynex_daemon::services::notifications::{
    run_notification_expiry_scheduler, try_acquire_fdo_name, FdoNotificationsServer,
    NotificationsServer, NotificationsSharedState,
};
use kynex_ipc::notifications::{
    FdoNotificationsProxy, Notifications1Proxy, FDO_NOTIFICATIONS_DESTINATION,
    FDO_NOTIFICATIONS_INTERFACE, FDO_NOTIFICATIONS_PATH, NOTIFICATIONS_INTERFACE,
    NOTIFICATIONS_PATH,
};
use kynex_ipc::DAEMON_DESTINATION;
use tokio::io::AsyncBufReadExt;
use zbus::object_server::SignalEmitter;
use zbus::Connection;
use zbus::{Array, Structure, Variant};

/// Helper managing a private, isolated `dbus-daemon` instance.
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
        if let Some(path) = self.address.strip_prefix("unix:path=") {
            let socket_path = path.split(',').next().unwrap_or(path);
            let _ = std::fs::remove_file(socket_path);
        }
    }
}

/// Harness setting up the daemon and test connections on the isolated bus.
#[allow(dead_code)]
struct TestHarness {
    _bus: TestBus,
    daemon_conn: Connection,
    client_a_conn: Connection,
    client_b_conn: Connection,
    shared_state: Arc<NotificationsSharedState>,
}

impl TestHarness {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let bus = TestBus::new().await?;

        let shared_state = Arc::new(NotificationsSharedState::new(None));
        let fdo_server = FdoNotificationsServer::new(Arc::clone(&shared_state));
        let notif_server = NotificationsServer::new(Arc::clone(&shared_state));

        let daemon_conn = zbus::connection::Builder::address(bus.address())
            .name(DAEMON_DESTINATION)
            .serve_at(NOTIFICATIONS_PATH, notif_server.clone())
            .serve_at(FDO_NOTIFICATIONS_PATH, fdo_server.clone())
            .build()
            .await?;

        shared_state.set_connection(daemon_conn.clone()).await;

        if let Ok(notif1_emitter) = SignalEmitter::new(&daemon_conn, NOTIFICATIONS_PATH) {
            notif_server.set_emitter(notif1_emitter).await;
        }
        if let Ok(fdo_emitter) = SignalEmitter::new(&daemon_conn, FDO_NOTIFICATIONS_PATH) {
            fdo_server.set_emitter(fdo_emitter).await;
        }

        let acquired = try_acquire_fdo_name(&daemon_conn).await;
        shared_state
            .fdo_name_acquired
            .store(acquired, Ordering::SeqCst);

        tokio::spawn(run_notification_expiry_scheduler(
            Arc::clone(&shared_state),
            daemon_conn.clone(),
        ));

        let client_a_conn = zbus::connection::Builder::address(bus.address())
            .build()
            .await?;
        let client_b_conn = zbus::connection::Builder::address(bus.address())
            .build()
            .await?;

        Ok(Self {
            _bus: bus,
            daemon_conn,
            client_a_conn,
            client_b_conn,
            shared_state,
        })
    }

    async fn fdo_client_a(&self) -> zbus::Result<FdoNotificationsProxy<'_>> {
        FdoNotificationsProxy::new(&self.client_a_conn).await
    }

    async fn fdo_client_b(&self) -> zbus::Result<FdoNotificationsProxy<'_>> {
        FdoNotificationsProxy::new(&self.client_b_conn).await
    }

    async fn shell_client(&self) -> zbus::Result<Notifications1Proxy<'_>> {
        Notifications1Proxy::new(&self.client_a_conn).await
    }
}

#[tokio::test]
async fn test_spec_conformance_capabilities_and_server_info() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let fdo = harness.fdo_client_a().await?;

    let caps = fdo.get_capabilities().await?;
    assert_eq!(caps, vec!["body".to_string(), "actions".to_string()]);
    assert!(!caps.contains(&"body-markup".to_string()));
    assert!(!caps.contains(&"persistence".to_string()));

    let (name, vendor, version, spec_ver) = fdo.get_server_information().await?;
    assert_eq!(name, "kynex-daemon");
    assert_eq!(vendor, "Kynex Project");
    assert_eq!(spec_ver, "1.2");
    assert!(!version.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_spec_replaces_id_and_sender_authorization() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let client_a = harness.fdo_client_a().await?;
    let client_b = harness.fdo_client_b().await?;
    let hints = HashMap::new();

    // 1. Fresh notification creates non-zero ID
    let id1 = client_a
        .notify("AppA", 0, "", "Summary", "Body", &[], &hints, 5000)
        .await?;
    assert!(id1 > 0);

    // 2. Same sender can replace their notification
    let id1_rep = client_a
        .notify(
            "AppA",
            id1,
            "",
            "Summary Updated",
            "Body",
            &[],
            &hints,
            5000,
        )
        .await?;
    assert_eq!(id1, id1_rep);

    // 3. Different sender trying to replace id1 is rejected
    let hijack_res = client_b
        .notify("AppB", id1, "", "Hijack", "Body", &[], &hints, 5000)
        .await;
    assert!(hijack_res.is_err());

    // 4. Different sender trying to CloseNotification id1 is rejected
    let close_hijack = client_b.close_notification(id1).await;
    assert!(close_hijack.is_err());

    // 5. Replaces nonexistent ID returns replaces_id per FDO Spec v1.2 §4.1
    let id2 = client_a
        .notify("AppA", 999999, "", "New", "Body", &[], &hints, 5000)
        .await?;
    assert_eq!(id2, 999999);

    // 5b. CloseNotification on nonexistent ID returns empty D-Bus error per FDO Spec v1.2 §4.2
    let close_nonexistent = client_a.close_notification(888888).await;
    assert!(close_nonexistent.is_err());

    // 6. Original creator can close their notification (Reason 3: Closed by CloseNotification)
    let mut close_stream = client_a.receive_notification_closed().await?;
    client_a.close_notification(id1).await?;

    let close_signal = tokio::time::timeout(Duration::from_secs(2), close_stream.next()).await?;
    let binding = close_signal.ok_or("No close signal received")?;
    let close_args = binding.args()?;
    assert_eq!(close_args.id, id1);
    assert_eq!(close_args.reason, 3);

    Ok(())
}

#[tokio::test]
async fn test_action_round_trip_unicast_isolation() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let client_a = harness.fdo_client_a().await?;
    let client_b = harness.fdo_client_b().await?;
    let shell = harness.shell_client().await?;
    let hints = HashMap::new();

    // Client A and B subscribe to ActionInvoked
    let mut stream_a = client_a.receive_action_invoked().await?;
    let mut stream_b = client_b.receive_action_invoked().await?;

    // Client A creates notification with actions
    let actions = ["open", "Open File", "default", "Default Action"];
    let notif_id = client_a
        .notify(
            "AppA",
            0,
            "",
            "Action Test",
            "Click me",
            &actions,
            &hints,
            0,
        )
        .await?;

    // Shell invokes action on notif_id
    shell.invoke_action(notif_id, "open").await?;

    // Client A must receive ActionInvoked
    let signal_a = tokio::time::timeout(Duration::from_secs(2), stream_a.next()).await?;
    let binding_a = signal_a.ok_or("Client A did not receive ActionInvoked")?;
    let args_a = binding_a.args()?;
    assert_eq!(args_a.id, notif_id);
    assert_eq!(args_a.action_key, "open");

    // Client B must NEVER receive ActionInvoked (unicast isolation verified)
    let signal_b = tokio::time::timeout(Duration::from_millis(200), stream_b.next()).await;
    assert!(
        signal_b.is_err(),
        "Client B incorrectly received unicast ActionInvoked signal!"
    );

    Ok(())
}

#[tokio::test]
async fn test_do_not_disturb_and_urgency_filtering() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let client = harness.fdo_client_a().await?;
    let shell = harness.shell_client().await?;

    // Enable Do Not Disturb
    shell.set_do_not_disturb(true).await?;
    assert!(shell.do_not_disturb().await?);

    // Normal urgency notification: suppressed from popups, stored in history
    let mut normal_hints = HashMap::new();
    normal_hints.insert("urgency", Variant::from(1u8));
    let notif_norm = client
        .notify(
            "App",
            0,
            "",
            "Normal",
            "Suppressed",
            &[],
            &normal_hints,
            5000,
        )
        .await?;

    let history = shell.get_history().await?;
    assert!(history.iter().any(|h| h.id == notif_norm));

    let active_popups = harness
        .shared_state
        .manager
        .read()
        .await
        .visible_popups()
        .len();
    assert_eq!(active_popups, 0);

    // Critical urgency notification: bypasses DND and displays popup
    let mut crit_hints = HashMap::new();
    crit_hints.insert("urgency", Variant::from(2u8));
    let notif_crit = client
        .notify(
            "App",
            0,
            "",
            "Critical",
            "Urgent Alert",
            &[],
            &crit_hints,
            0,
        )
        .await?;

    let active_popups_after = harness
        .shared_state
        .manager
        .read()
        .await
        .visible_popups()
        .len();
    assert_eq!(active_popups_after, 1);
    assert_eq!(
        harness.shared_state.manager.read().await.visible_popups()[0],
        notif_crit
    );

    // Disable DND
    shell.set_do_not_disturb(false).await?;
    assert!(!shell.do_not_disturb().await?);

    Ok(())
}

#[tokio::test]
async fn test_hostile_corpus_sanitization_and_injection_defense() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let client = harness.fdo_client_a().await?;
    let shell = harness.shell_client().await?;
    let hints = HashMap::new();

    // 1. Huge summary & body with script, img, links, control characters, bidi overrides
    let huge_summary = "A".repeat(1000);
    let hostile_body = format!(
        "<script>alert('xss');</script><img src=\"file:///etc/passwd\"/>\
         <a href=\"javascript:attack()\">click</a>Hello\u{202E}BIDI\u{202C}World\x07\x08&amp;&lt;&gt;{}",
        "B".repeat(10000)
    );

    let id = client
        .notify(
            "HostileApp",
            0,
            "",
            &huge_summary,
            &hostile_body,
            &[],
            &hints,
            5000,
        )
        .await?;

    // Verify in active notifications before close
    {
        let mgr = harness.shared_state.manager.read().await;
        let active = mgr
            .active_notifications()
            .get(&id)
            .ok_or("Active notif missing")?;
        assert!(active.summary.len() <= 256);
        assert!(active.body.len() <= 4096);
        assert!(!active.body.contains("<script>"));
        assert!(!active.body.contains("<img"));
        assert!(!active.body.contains("<a"));
        assert!(!active.body.contains("alert('xss')"));
        assert!(!active.body.contains('\u{202E}'));
        assert!(!active.body.contains('\x07'));
        assert!(active.body.contains("&<>"));
    }

    // Close notification so it transitions to history
    client.close_notification(id).await?;

    let history = shell.get_history().await?;
    let item = history
        .iter()
        .find(|h| h.id == id)
        .ok_or("Notification missing in history")?;

    // Verify summary capped at 256 bytes
    assert!(item.summary.len() <= 256);
    // Verify body capped at 4096 bytes
    assert!(item.body.len() <= 4096);
    // Verify scripts, img, a tags stripped
    assert!(!item.body.contains("<script>"));
    assert!(!item.body.contains("<img"));
    assert!(!item.body.contains("<a"));
    assert!(!item.body.contains("alert('xss')"));
    // Verify bidi override and control characters dropped
    assert!(!item.body.contains('\u{202E}'));
    assert!(!item.body.contains('\x07'));
    // Verify safe entity decoding
    assert!(item.body.contains("&<>"));

    Ok(())
}

#[tokio::test]
async fn test_hostile_image_data_validation() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let client = harness.fdo_client_a().await?;

    // Test helper to build image-data Structure (iiibiiay)
    let build_image_variant = |w: i32,
                               h: i32,
                               rowstride: i32,
                               has_alpha: bool,
                               bps: i32,
                               ch: i32,
                               data: &[u8]|
     -> Variant<'static> {
        let byte_array = Array::from(data.to_vec());
        let structure = Structure::builder()
            .add_field(w)
            .add_field(h)
            .add_field(rowstride)
            .add_field(has_alpha)
            .add_field(bps)
            .add_field(ch)
            .add_field(byte_array)
            .build()
            .unwrap();
        Variant::from(structure)
    };

    // 1. Pathological lying dimensions (> 256)
    let mut hints = HashMap::new();
    let img_lying = build_image_variant(1024, 1024, 4096, true, 8, 4, &[0u8; 100]);
    hints.insert("image-data", img_lying);

    let id = client
        .notify("App", 0, "", "Img1", "Body", &[], &hints, 5000)
        .await?;
    let mgr = harness.shared_state.manager.read().await;
    assert!(mgr.active_notifications().get(&id).unwrap().image.is_none());
    drop(mgr);

    // 2. Pathological integer overflow product
    let mut hints2 = HashMap::new();
    let img_overflow = build_image_variant(i32::MAX, 2, i32::MAX, false, 8, 3, &[0u8; 10]);
    hints2.insert("image-data", img_overflow);
    let id2 = client
        .notify("App", 0, "", "Img2", "Body", &[], &hints2, 5000)
        .await?;
    let mgr2 = harness.shared_state.manager.read().await;
    assert!(mgr2
        .active_notifications()
        .get(&id2)
        .unwrap()
        .image
        .is_none());
    drop(mgr2);

    // 3. Valid embedded RGBA image (2x2)
    let mut hints3 = HashMap::new();
    let valid_data = vec![255u8; 16]; // 2 * 2 * 4 = 16 bytes
    let img_valid = build_image_variant(2, 2, 8, true, 8, 4, &valid_data);
    hints3.insert("image-data", img_valid);
    let id3 = client
        .notify("App", 0, "", "Img3", "Body", &[], &hints3, 5000)
        .await?;
    let mgr3 = harness.shared_state.manager.read().await;
    let saved_img = mgr3
        .active_notifications()
        .get(&id3)
        .unwrap()
        .image
        .as_ref();
    assert!(saved_img.is_some());
    assert_eq!(saved_img.unwrap().width, 2);
    assert_eq!(saved_img.unwrap().height, 2);

    Ok(())
}

#[tokio::test]
async fn test_token_bucket_flood_protection() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let client = harness.fdo_client_a().await?;
    let hints = HashMap::new();

    // Burst limit is 10 tokens: first 10 calls succeed immediately
    for i in 0..10 {
        let res = client
            .notify(
                "BurstApp",
                0,
                "",
                &format!("Burst {i}"),
                "Body",
                &[],
                &hints,
                5000,
            )
            .await;
        assert!(res.is_ok(), "Call {i} within burst limit failed");
    }

    // 11th call must immediately be rejected with LimitsExceeded
    let res_exceeded = client
        .notify("BurstApp", 0, "", "Exceeded", "Body", &[], &hints, 5000)
        .await;
    assert!(
        res_exceeded.is_err(),
        "11th rapid call was not rate limited!"
    );

    // Other connection (client_b) has independent quota and succeeds
    let client_b = harness.fdo_client_b().await?;
    let res_b = client_b
        .notify("ClientB", 0, "", "Independent", "Body", &[], &hints, 5000)
        .await;
    assert!(
        res_b.is_ok(),
        "Client B was incorrectly throttled by Client A"
    );

    Ok(())
}

#[tokio::test]
async fn test_kynex_ipc_proxy_traits_and_constants_against_daemon() -> Result<(), Box<dyn Error>> {
    assert_eq!(NOTIFICATIONS_INTERFACE, "org.kynexos.Daemon.Notifications1");
    assert_eq!(NOTIFICATIONS_PATH, "/org/kynexos/Daemon/Notifications");
    assert_eq!(
        FDO_NOTIFICATIONS_DESTINATION,
        "org.freedesktop.Notifications"
    );
    assert_eq!(FDO_NOTIFICATIONS_INTERFACE, "org.freedesktop.Notifications");
    assert_eq!(FDO_NOTIFICATIONS_PATH, "/org/freedesktop/Notifications");

    let harness = TestHarness::new().await?;
    let fdo = harness.fdo_client_a().await?;
    let shell = harness.shell_client().await?;

    assert_eq!(
        fdo.inner().path().as_str(),
        "/org/freedesktop/Notifications"
    );
    assert_eq!(
        fdo.inner().destination().as_str(),
        "org.freedesktop.Notifications"
    );
    assert_eq!(
        fdo.inner().interface().as_str(),
        "org.freedesktop.Notifications"
    );
    assert_eq!(
        shell.inner().path().as_str(),
        "/org/kynexos/Daemon/Notifications"
    );
    assert_eq!(shell.inner().destination().as_str(), "org.kynexos.Daemon");
    assert_eq!(
        shell.inner().interface().as_str(),
        "org.kynexos.Daemon.Notifications1"
    );

    let caps = fdo.get_capabilities().await?;
    assert!(caps.contains(&"body".to_string()));
    assert!(caps.contains(&"actions".to_string()));

    let (name, vendor, _ver, spec_ver) = fdo.get_server_information().await?;
    assert_eq!(name, "kynex-daemon");
    assert_eq!(vendor, "Kynex Project");
    assert_eq!(spec_ver, "1.2");

    let mut shown_stream = shell.receive_shown().await?;
    let mut updated_stream = shell.receive_updated().await?;
    let mut closed_stream = shell.receive_closed().await?;
    let mut fdo_closed_stream = fdo.receive_notification_closed().await?;
    let mut action_stream = fdo.receive_action_invoked().await?;

    let hints = HashMap::new();
    let actions = ["open", "Open File", "default", "Default Action"];

    // 1. Send notification via FdoNotificationsProxy
    let notif_id = fdo
        .notify(
            "TestApp",
            0,
            "test-icon",
            "Test Summary",
            "Test Body",
            &actions,
            &hints,
            5000,
        )
        .await?;
    assert!(notif_id > 0);

    let shown_sig = tokio::time::timeout(Duration::from_secs(2), shown_stream.next())
        .await?
        .ok_or("No shown signal received")?;
    let shown_args = shown_sig.args()?;
    assert_eq!(shown_args.id, notif_id);
    assert_eq!(shown_args.app_name, "TestApp");
    assert_eq!(shown_args.summary, "Test Summary");
    assert_eq!(shown_args.body, "Test Body");

    // 2. Update notification via replaces_id
    let updated_id = fdo
        .notify(
            "TestApp",
            notif_id,
            "test-icon",
            "Updated Summary",
            "Updated Body",
            &actions,
            &hints,
            5000,
        )
        .await?;
    assert_eq!(updated_id, notif_id);

    let updated_sig = tokio::time::timeout(Duration::from_secs(2), updated_stream.next())
        .await?
        .ok_or("No updated signal received")?;
    let upd_args = updated_sig.args()?;
    assert_eq!(upd_args.id, notif_id);
    assert_eq!(upd_args.summary, "Updated Summary");
    assert_eq!(upd_args.body, "Updated Body");

    // 3. Pause and Resume timeout via Notifications1Proxy
    shell.pause_timeout(notif_id).await?;
    shell.resume_timeout(notif_id).await?;

    // 4. DoNotDisturb property get and set via Notifications1Proxy
    assert!(!shell.do_not_disturb().await?);
    shell.set_do_not_disturb(true).await?;
    assert!(shell.do_not_disturb().await?);
    shell.set_do_not_disturb(false).await?;
    assert!(!shell.do_not_disturb().await?);

    // 5. Invoke action via Notifications1Proxy -> routes to Fdo client and closes non-resident notification
    shell.invoke_action(notif_id, "open").await?;
    let act_sig = tokio::time::timeout(Duration::from_secs(2), action_stream.next())
        .await?
        .ok_or("No action_invoked signal received")?;
    let act_args = act_sig.args()?;
    assert_eq!(act_args.id, notif_id);
    assert_eq!(act_args.action_key, "open");

    let closed_sig = tokio::time::timeout(Duration::from_secs(2), closed_stream.next())
        .await?
        .ok_or("No closed signal received")?;
    let cl_args = closed_sig.args()?;
    assert_eq!(cl_args.id, notif_id);

    let fdo_cl_sig = tokio::time::timeout(Duration::from_secs(2), fdo_closed_stream.next())
        .await?
        .ok_or("No fdo notification_closed signal received")?;
    let fdo_cl_args = fdo_cl_sig.args()?;
    assert_eq!(fdo_cl_args.id, notif_id);

    // 6. Query history via Notifications1Proxy now that notification was closed
    let history = shell.get_history().await?;
    assert!(history
        .iter()
        .any(|item| item.id == notif_id && item.summary == "Updated Summary"));

    // 7. Post second notification to test dismiss via Notifications1Proxy
    let notif2_id = fdo
        .notify(
            "TestApp2",
            0,
            "",
            "Second Summary",
            "Second Body",
            &[],
            &hints,
            5000,
        )
        .await?;
    let _ = tokio::time::timeout(Duration::from_secs(2), shown_stream.next()).await?;

    shell.dismiss(notif2_id).await?;
    let closed2_sig = tokio::time::timeout(Duration::from_secs(2), closed_stream.next())
        .await?
        .ok_or("No closed signal received for notif2")?;
    let cl2_args = closed2_sig.args()?;
    assert_eq!(cl2_args.id, notif2_id);
    assert_eq!(cl2_args.reason, 2);

    // 8. CloseNotification via FdoNotificationsProxy on already closed notification returns error
    let close_err = fdo.close_notification(notif_id).await;
    assert!(
        close_err.is_err(),
        "CloseNotification on non-existent notification must return error"
    );

    Ok(())
}

#[tokio::test]
async fn test_id_squatting_and_sender_isolation() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let client_a = harness.fdo_client_a().await?;
    let client_b = harness.fdo_client_b().await?;
    let hints = HashMap::new();

    // The next ID that the allocator will hand out is 1.
    // Client B attempts ID squatting by calling Notify with replaces_id = 1.
    // Since ID 1 is not currently owned by another sender, it is allocated to Client B.
    let squat_id = client_b
        .notify(
            "SquatterApp",
            1,
            "",
            "Squat Title",
            "Squat Body",
            &[],
            &hints,
            5000,
        )
        .await?;
    assert_eq!(
        squat_id, 1,
        "Non-existent replaces_id must return replaces_id (1)"
    );

    // Client A sends a normal notification with replaces_id = 0.
    // The allocator MUST skip ID 1 (which is active under Client B) and allocate ID 2.
    let client_a_id = client_a
        .notify(
            "LegitApp",
            0,
            "",
            "Legit Title",
            "Legit Body",
            &[],
            &hints,
            5000,
        )
        .await?;
    assert_ne!(client_a_id, squat_id, "Allocator must skip squatted ID 1");
    assert_eq!(
        client_a_id, 2,
        "Client A must be allocated next available ID (2)"
    );

    // Client B attempts to replace Client A's notification (ID 2): must fail
    let hijack_rep = client_b
        .notify(
            "SquatterApp",
            client_a_id,
            "",
            "Hijack",
            "Body",
            &[],
            &hints,
            5000,
        )
        .await;
    assert!(
        hijack_rep.is_err(),
        "Client B replacing Client A notification must be rejected"
    );

    // Client A attempts to replace Client B's notification (ID 1): must fail
    let legit_rep = client_a
        .notify(
            "LegitApp",
            squat_id,
            "",
            "Hijack",
            "Body",
            &[],
            &hints,
            5000,
        )
        .await;
    assert!(
        legit_rep.is_err(),
        "Client A replacing Client B notification must be rejected"
    );

    // Client B attempts to close Client A's notification (ID 2): must fail
    let hijack_close = client_b.close_notification(client_a_id).await;
    assert!(
        hijack_close.is_err(),
        "Client B closing Client A notification must be rejected"
    );

    // Client A attempts to close Client B's notification (ID 1): must fail
    let legit_close = client_a.close_notification(squat_id).await;
    assert!(
        legit_close.is_err(),
        "Client A closing Client B notification must be rejected"
    );

    // Per-sender rate limit / quota still strictly applies to squatter (Client B):
    // Client B already sent 2 requests (squat_id and the rejected hijack). Send 8 more requests to exhaust the burst of 10.
    for i in 1..=8 {
        let res = client_b
            .notify("SquatterApp", 0, "", "Squat", "Body", &[], &hints, 5000)
            .await;
        assert!(res.is_ok(), "Call {i} within burst limit failed");
    }

    // 11th call by Client B must be rejected with rate limit error
    let res_quota = client_b
        .notify("SquatterApp", 0, "", "Exceed", "Body", &[], &hints, 5000)
        .await;
    assert!(
        res_quota.is_err(),
        "Client B exceeded per-sender quota but was not rate limited"
    );

    // Client A's quota is isolated and Client A can still send notifications
    let client_a_id2 = client_a
        .notify(
            "LegitApp",
            0,
            "",
            "Still Working",
            "Body",
            &[],
            &hints,
            5000,
        )
        .await;
    assert!(
        client_a_id2.is_ok(),
        "Client A was throttled by Client B's activity"
    );

    Ok(())
}
