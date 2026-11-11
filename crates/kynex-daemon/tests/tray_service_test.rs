//! Integration tests for the System Tray subsystem using an isolated `dbus-daemon`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::unreadable_literal,
    clippy::doc_markdown,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::assert_is_empty,
    clippy::unused_async,
    clippy::type_complexity,
    clippy::unused_async_trait_impl,
    clippy::used_underscore_binding
)]

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use kynex_daemon::services::tray::{
    run_tray_dbus_name_watcher, try_acquire_watcher_names, FdoStatusNotifierWatcherServer,
    StatusNotifierWatcherServer, TrayServer, TraySharedState,
};
use kynex_ipc::tray::fdo::FdoStatusNotifierWatcherProxy;
use kynex_ipc::tray::{
    StatusNotifierWatcherProxy, Tray1Proxy, STATUS_NOTIFIER_WATCHER_DESTINATION_FDO,
    STATUS_NOTIFIER_WATCHER_DESTINATION_KDE, STATUS_NOTIFIER_WATCHER_PATH, TRAY_PATH,
};
use kynex_ipc::DAEMON_DESTINATION;
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection};

/// Private, isolated `dbus-daemon` session for integration testing.
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

/// Fake StatusNotifierItem implementing `org.kde.StatusNotifierItem`.
#[derive(Clone, Default)]
struct FakeItem {
    title: Arc<RwLock<String>>,
    icon_name: Arc<RwLock<String>>,
    icon_theme_path: Arc<RwLock<String>>,
    icon_pixmap: Arc<RwLock<Vec<(i32, i32, Vec<u8>)>>>,
    status: Arc<RwLock<String>>,
    tooltip: Arc<RwLock<(String, Vec<(i32, i32, Vec<u8>)>, String, String)>>,
    hang_property: Arc<AtomicBool>,
    delay_ms: Arc<RwLock<u64>>,
    attention_icon_name: Arc<RwLock<String>>,
    attention_icon_pixmap: Arc<RwLock<Vec<(i32, i32, Vec<u8>)>>>,
    last_activated: Arc<RwLock<Option<(i32, i32)>>>,
    last_secondary_activated: Arc<RwLock<Option<(i32, i32)>>>,
    last_context_menu: Arc<RwLock<Option<(i32, i32)>>>,
    last_scroll: Arc<RwLock<Option<(i32, String)>>>,
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl FakeItem {
    async fn context_menu(&self, x: i32, y: i32) {
        *self.last_context_menu.write().await = Some((x, y));
    }

    async fn activate(&self, x: i32, y: i32) {
        *self.last_activated.write().await = Some((x, y));
    }

    async fn secondary_activate(&self, x: i32, y: i32) {
        *self.last_secondary_activated.write().await = Some((x, y));
    }

    async fn scroll(&self, delta: i32, orientation: &str) {
        *self.last_scroll.write().await = Some((delta, orientation.to_string()));
    }

    #[zbus(property)]
    async fn category(&self) -> String {
        "ApplicationStatus".to_string()
    }

    #[zbus(property)]
    async fn id(&self) -> String {
        "fake-item".to_string()
    }

    #[zbus(property)]
    async fn title(&self) -> String {
        let delay = *self.delay_ms.read().await;
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        if self.hang_property.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        self.title.read().await.clone()
    }

    #[zbus(property)]
    async fn status(&self) -> String {
        self.status.read().await.clone()
    }

    #[zbus(property)]
    async fn window_id(&self) -> i32 {
        42
    }

    #[zbus(property)]
    async fn icon_name(&self) -> String {
        self.icon_name.read().await.clone()
    }

    #[zbus(property)]
    async fn icon_theme_path(&self) -> String {
        self.icon_theme_path.read().await.clone()
    }

    #[zbus(property)]
    async fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        self.icon_pixmap.read().await.clone()
    }

    #[zbus(property)]
    async fn overlay_icon_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    async fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        Vec::new()
    }

    #[zbus(property)]
    async fn attention_icon_name(&self) -> String {
        self.attention_icon_name.read().await.clone()
    }

    #[zbus(property)]
    async fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        self.attention_icon_pixmap.read().await.clone()
    }

    #[zbus(property)]
    async fn attention_movie_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    async fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        self.tooltip.read().await.clone()
    }

    #[zbus(property)]
    async fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn menu(&self) -> zbus::OwnedObjectPath {
        zbus::OwnedObjectPath::default()
    }

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_attention_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_overlay_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;
}

#[allow(dead_code)]
struct TestHarness {
    _bus: TestBus,
    daemon_conn: Connection,
    client_a_conn: Connection,
    client_b_conn: Connection,
    shared_state: Arc<TraySharedState>,
}

impl TestHarness {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let bus = TestBus::new().await?;
        let shared_state = Arc::new(TraySharedState::new());

        let kde_watcher = StatusNotifierWatcherServer::new(Arc::clone(&shared_state));
        let fdo_watcher = FdoStatusNotifierWatcherServer::new(Arc::clone(&shared_state));
        let tray_server = TrayServer::new(Arc::clone(&shared_state));
        let daemon_server =
            kynex_daemon::server::DaemonServer::new(Arc::new(AtomicBool::new(true)));

        let daemon_conn = zbus::connection::Builder::address(bus.address())
            .name(DAEMON_DESTINATION)
            .serve_at(kynex_ipc::DAEMON_PATH, daemon_server)
            .serve_at(STATUS_NOTIFIER_WATCHER_PATH, kde_watcher.clone())
            .serve_at(STATUS_NOTIFIER_WATCHER_PATH, fdo_watcher.clone())
            .serve_at(TRAY_PATH, tray_server.clone())
            .build()
            .await?;

        shared_state.set_connection(daemon_conn.clone()).await;

        let kde_emitter = SignalEmitter::new(&daemon_conn, STATUS_NOTIFIER_WATCHER_PATH)?;
        kde_watcher.set_emitter(kde_emitter).await;
        let fdo_emitter = SignalEmitter::new(&daemon_conn, STATUS_NOTIFIER_WATCHER_PATH)?;
        fdo_watcher.set_emitter(fdo_emitter).await;
        let tray_emitter = SignalEmitter::new(&daemon_conn, TRAY_PATH)?;
        tray_server.set_emitter(tray_emitter).await;

        let (kde_acq, fdo_acq) = try_acquire_watcher_names(&daemon_conn).await;
        shared_state
            .kde_name_acquired
            .store(kde_acq, Ordering::SeqCst);
        shared_state
            .fdo_name_acquired
            .store(fdo_acq, Ordering::SeqCst);

        tokio::spawn(run_tray_dbus_name_watcher(
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

    async fn kde_watcher_client(&self) -> zbus::Result<StatusNotifierWatcherProxy<'_>> {
        StatusNotifierWatcherProxy::builder(&self.client_a_conn)
            .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
            .path(STATUS_NOTIFIER_WATCHER_PATH)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
    }

    async fn fdo_watcher_client(&self) -> zbus::Result<FdoStatusNotifierWatcherProxy<'_>> {
        FdoStatusNotifierWatcherProxy::builder(&self.client_a_conn)
            .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_FDO)
            .path(STATUS_NOTIFIER_WATCHER_PATH)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
    }

    async fn tray1_client(&self) -> zbus::Result<Tray1Proxy<'_>> {
        Tray1Proxy::builder(&self.client_a_conn)
            .destination(DAEMON_DESTINATION)
            .path(TRAY_PATH)
            .build()
            .await
    }

    async fn core1_client(&self) -> zbus::Result<kynex_ipc::Core1Proxy<'_>> {
        kynex_ipc::Core1Proxy::builder(&self.client_b_conn)
            .destination(DAEMON_DESTINATION)
            .path(kynex_ipc::DAEMON_PATH)
            .build()
            .await
    }
}

#[tokio::test]
async fn test_watcher_spec_conformance_and_properties() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;

    // ProtocolVersion
    assert_eq!(watcher.protocol_version().await?, 0);

    // Initial state
    assert!(!watcher.is_status_notifier_host_registered().await?);
    assert!(watcher.registered_status_notifier_items().await?.is_empty());

    // Register host on dedicated connection that will vanish
    let host_conn = zbus::connection::Builder::address(harness._bus.address())
        .build()
        .await?;
    let host_watcher = StatusNotifierWatcherProxy::builder(&host_conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .build()
        .await?;
    host_watcher
        .register_status_notifier_host("kynex-shell")
        .await?;
    assert!(watcher.is_status_notifier_host_registered().await?);

    // Also test FDO alias watcher endpoint
    let fdo_watcher = harness.fdo_watcher_client().await?;
    assert_eq!(fdo_watcher.protocol_version().await?, 0);
    assert!(fdo_watcher.is_status_notifier_host_registered().await?);

    Ok(())
}

#[tokio::test]
async fn test_spoof_detection_and_rejection() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let _watcher = harness.kde_watcher_client().await?;

    // Client B tries to register Client A's name
    let client_b_watcher = StatusNotifierWatcherProxy::builder(&harness.client_b_conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .build()
        .await?;

    let client_a_bus_name = harness.client_a_conn.unique_name().unwrap().as_str();

    // 1. Spoof with Client A's unique name
    let spoof_arg = format!("{client_a_bus_name}/StatusNotifierItem");
    let res = client_b_watcher
        .register_status_notifier_item(&spoof_arg)
        .await;
    assert!(
        res.is_err(),
        "Registering another client's unique bus name must be rejected"
    );

    // 2. Spoof with unowned well-known name
    let res_unowned = client_b_watcher
        .register_status_notifier_item("org.fake.UnownedService")
        .await;
    assert!(
        res_unowned.is_err(),
        "Registering unowned service name must be rejected"
    );

    Ok(())
}

#[tokio::test]
async fn test_capacity_limits_per_sender_and_total() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;

    // Serve 4 items on Client A connection at distinct paths
    for i in 1..=4 {
        let path = format!("/Item{i}");
        let item = FakeItem::default();
        *item.title.write().await = format!("Item {i}");
        *item.status.write().await = "Active".to_string();

        harness
            .client_a_conn
            .object_server()
            .at(path.as_str(), item)
            .await?;
        watcher.register_status_notifier_item(path.as_str()).await?;
    }

    assert_eq!(watcher.registered_status_notifier_items().await?.len(), 4);

    // 5th item from same sender must be rejected (max 4 per sender)
    let path5 = "/Item5";
    let item5 = FakeItem::default();
    harness
        .client_a_conn
        .object_server()
        .at(path5, item5)
        .await?;
    let res5 = watcher.register_status_notifier_item(path5).await;
    assert!(res5.is_err(), "5th item from same sender must be rejected");

    Ok(())
}

#[tokio::test]
async fn test_name_vanish_unregisters_item_and_host() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    // Register host on dedicated connection that will vanish
    let host_conn = zbus::connection::Builder::address(harness._bus.address())
        .build()
        .await?;
    let host_watcher = StatusNotifierWatcherProxy::builder(&host_conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .build()
        .await?;
    host_watcher
        .register_status_notifier_host("kynex-shell")
        .await?;
    assert!(watcher.is_status_notifier_host_registered().await?);

    // Create item on dedicated connection that we will close
    let item_conn = zbus::connection::Builder::address(harness._bus.address())
        .build()
        .await?;
    let item = FakeItem::default();
    *item.title.write().await = "Ephemeral Item".to_string();
    *item.status.write().await = "Active".to_string();
    item_conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await?;

    let item_watcher = StatusNotifierWatcherProxy::builder(&item_conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .build()
        .await?;

    item_watcher
        .register_status_notifier_item("/StatusNotifierItem")
        .await?;

    // Verify item appeared
    assert_eq!(watcher.registered_status_notifier_items().await?.len(), 1);
    assert_eq!(tray1.get_items().await?.len(), 1);

    // Close item connection
    let _ = item_conn.close().await;

    // Wait for NameOwnerChanged to trigger removal
    let mut cleared = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if watcher.registered_status_notifier_items().await?.is_empty() {
            cleared = true;
            break;
        }
    }
    assert!(cleared, "Item must be removed after its owner disconnects");
    assert_eq!(tray1.get_items().await?.len(), 0);

    // Close host connection and verify last host vanish
    let _ = host_conn.close().await;
    let mut host_cleared = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if !watcher.is_status_notifier_host_registered().await? {
            host_cleared = true;
            break;
        }
    }
    assert!(
        host_cleared,
        "Host must be unregistered after connection closes"
    );

    Ok(())
}

#[tokio::test]
async fn test_hostile_pixmap_corpus() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    let item = FakeItem::default();
    *item.title.write().await = "Hostile Pixmap App".to_string();
    *item.status.write().await = "Active".to_string();

    // 1. Lying dimensions: 64x64 claimed but data is only 10 bytes
    // 2. Negative dimensions: (-10, 10, data)
    // 3. Huge dimensions: (1000, 1000, 4MB)
    // 4. Arithmetic overflow product: (65536, 65536, data)
    *item.icon_pixmap.write().await = vec![
        (64, 64, vec![0u8; 10]),
        (-10, 10, vec![0u8; 40]),
        (1000, 1000, vec![0u8; 4000]),
        (65536, 65536, vec![0u8; 16]),
    ];

    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await?;
    watcher
        .register_status_notifier_item("/StatusNotifierItem")
        .await?;

    let items = tray1.get_items().await?;
    assert_eq!(items.len(), 1);
    // All hostile pixmaps rejected safely; no crash, pixmap is empty
    assert!(items[0].icon_pixmap.is_empty());
    assert_eq!(items[0].icon_pixmap_width, 0);
    assert_eq!(items[0].icon_pixmap_height, 0);

    Ok(())
}

#[tokio::test]
async fn test_hostile_string_sanitization() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    let item = FakeItem::default();
    let hostile_title = format!(
        "<script>alert(1)</script>\u{202E}BidiText\u{202C}\x01\x02{}",
        "A".repeat(10_000)
    );
    let hostile_tooltip = format!("<b>Bold</b>\u{2066}Isolate\u{2069}{}", "T".repeat(10_000));

    *item.title.write().await = hostile_title;
    *item.status.write().await = "Active".to_string();
    *item.tooltip.write().await = (
        "icon".to_string(),
        Vec::new(),
        "ToolTip Title".to_string(),
        hostile_tooltip,
    );

    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await?;
    watcher
        .register_status_notifier_item("/StatusNotifierItem")
        .await?;

    let items = tray1.get_items().await?;
    assert_eq!(items.len(), 1);

    // Title length <= 128, stripped markup and bidi
    assert!(items[0].title.len() <= 128);
    assert!(!items[0].title.contains("<script>"));
    assert!(!items[0].title.contains('\u{202E}'));
    assert!(!items[0].title.contains('\x01'));

    // Tooltip text length <= 512, stripped markup and bidi
    assert!(items[0].tooltip_text.len() <= 512);
    assert!(!items[0].tooltip_text.contains("<b>"));
    assert!(!items[0].tooltip_text.contains('\u{2066}'));

    Ok(())
}

#[tokio::test]
async fn test_hanging_item_timeout() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;

    let item = FakeItem::default();
    item.hang_property.store(true, Ordering::SeqCst);

    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await?;

    let start = std::time::Instant::now();
    // Daemon timeout is 2 seconds (PROPERTY_TIMEOUT_SECS); registering shouldn't hang for 30s
    let _ = watcher
        .register_status_notifier_item("/StatusNotifierItem")
        .await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "Hanging item property fetch must time out around 2s, took {elapsed:?}"
    );

    Ok(())
}

#[tokio::test]
async fn test_signal_flood_rate_limit() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    let item = FakeItem::default();
    *item.title.write().await = "Flood Item".to_string();
    *item.status.write().await = "Active".to_string();

    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await?;
    watcher
        .register_status_notifier_item("/StatusNotifierItem")
        .await?;

    let mut changed_stream = tray1.receive_item_changed().await?;

    let emitter = SignalEmitter::new(&harness.client_a_conn, "/StatusNotifierItem")?;
    // Flood 100 NewIcon signals
    for _ in 0..100 {
        let _ = FakeItem::new_icon(&emitter).await;
    }

    // Give daemon event loop time to process
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Drain received signals - token bucket allows initial burst of 10, not 100
    let mut received = 0;
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_millis(50), changed_stream.next()).await
    {
        received += 1;
    }

    assert!(
        received <= 20,
        "Rate limiter must bound received ItemChanged events, got {received}"
    );

    Ok(())
}

#[tokio::test]
async fn test_tray1_actions_dispatch() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    let item = FakeItem::default();
    *item.title.write().await = "Interactive App".to_string();
    *item.status.write().await = "Active".to_string();

    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item.clone())
        .await?;
    watcher
        .register_status_notifier_item("/StatusNotifierItem")
        .await?;

    let items = tray1.get_items().await?;
    assert_eq!(items.len(), 1);
    let id = &items[0].id;

    // Activate
    tray1.activate(id, 100, 200).await?;
    assert_eq!(*item.last_activated.read().await, Some((100, 200)));

    // SecondaryActivate
    tray1.secondary_activate(id, 150, 250).await?;
    assert_eq!(
        *item.last_secondary_activated.read().await,
        Some((150, 250))
    );

    // ContextMenu
    tray1.context_menu(id, 300, 400).await?;
    assert_eq!(*item.last_context_menu.read().await, Some((300, 400)));

    // Scroll
    tray1.scroll(id, 120, "vertical").await?;
    assert_eq!(
        *item.last_scroll.read().await,
        Some((120, "vertical".to_string()))
    );

    Ok(())
}

#[tokio::test]
async fn test_tray_signals_and_attention_state() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let fdo_watcher = harness.fdo_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    let item = FakeItem::default();
    *item.title.write().await = "Attention Item".to_string();
    *item.status.write().await = "NeedsAttention".to_string();
    *item.attention_icon_name.write().await = "mail-unread".to_string();
    *item.attention_icon_pixmap.write().await = vec![(2, 2, vec![255; 16])];
    *item.tooltip.write().await = (
        "mail-unread".to_string(),
        Vec::new(),
        "New Alerts".to_string(),
        "3 unread".to_string(),
    );

    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item.clone())
        .await?;
    fdo_watcher
        .register_status_notifier_item("/StatusNotifierItem")
        .await?;

    let items = tray1.get_items().await?;
    assert_eq!(items.len(), 1);
    assert!(items[0].has_attention);
    assert_eq!(items[0].attention_icon_name, "mail-unread");
    assert_eq!(items[0].attention_pixmap_width, 2);
    assert_eq!(items[0].attention_pixmap_height, 2);

    let emitter = SignalEmitter::new(&harness.client_a_conn, "/StatusNotifierItem")?;

    // Update title
    *item.title.write().await = "Attention Item V2".to_string();
    FakeItem::new_title(&emitter).await?;

    // Update status
    *item.status.write().await = "Active".to_string();
    FakeItem::new_status(&emitter, "Active").await?;

    // Update tooltip
    FakeItem::new_tool_tip(&emitter).await?;

    // Update attention icon
    FakeItem::new_attention_icon(&emitter).await?;

    // Update overlay icon
    FakeItem::new_overlay_icon(&emitter).await?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let updated_items = tray1.get_items().await?;
    assert_eq!(updated_items.len(), 1);
    assert_eq!(updated_items[0].title, "Attention Item V2");

    Ok(())
}

#[tokio::test]
async fn test_invalid_item_id_dispatch() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let tray1 = harness.tray1_client().await?;

    // Invalid item IDs
    assert!(tray1.activate("no_slash_id", 0, 0).await.is_err());
    assert!(tray1.secondary_activate("no_slash_id", 0, 0).await.is_err());
    assert!(tray1.context_menu("no_slash_id", 0, 0).await.is_err());
    assert!(tray1.scroll("no_slash_id", 0, "vertical").await.is_err());

    Ok(())
}

#[tokio::test]
async fn test_fdo_watcher_and_item_state_pixmaps() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let fdo_watcher = harness.fdo_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    // Verify properties on FDO watcher
    let items = fdo_watcher.registered_status_notifier_items().await?;
    assert!(items.is_empty());
    let host_reg = fdo_watcher.is_status_notifier_host_registered().await?;
    assert!(!host_reg);
    let ver = fdo_watcher.protocol_version().await?;
    assert_eq!(ver, 0);

    // Register host via FDO watcher
    fdo_watcher
        .register_status_notifier_host("org.freedesktop.StatusNotifierHost-test")
        .await?;
    assert!(fdo_watcher.is_status_notifier_host_registered().await?);

    // Create item with valid and invalid icon theme path and hostile attention pixmap
    let item = FakeItem::default();
    *item.title.write().await = "Themed App".to_string();
    *item.icon_theme_path.write().await = "/usr/share/icons/custom".to_string();
    *item.icon_pixmap.write().await = vec![(2, 2, vec![200; 16])];
    // Hostile attention pixmap that fails validation (e.g. invalid length)
    *item.attention_icon_pixmap.write().await = vec![(10, 10, vec![1, 2, 3])];

    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item.clone())
        .await?;
    fdo_watcher
        .register_status_notifier_item("/StatusNotifierItem")
        .await?;

    let tray_items = tray1.get_items().await?;
    assert_eq!(tray_items.len(), 1);
    assert_eq!(tray_items[0].icon_theme_path, "/usr/share/icons/custom");
    assert_eq!(tray_items[0].icon_pixmap_width, 2);
    // Hostile attention pixmap should have been rejected
    assert_eq!(tray_items[0].attention_pixmap_width, 0);

    // Test item_state_to_info
    let info = kynex_daemon::services::tray::item_state_to_info(&kynex_tray::ItemState {
        key: kynex_tray::ItemKey::new(":1.10".to_string(), "/test".to_string()),
        item_id: "test".to_string(),
        title: "Test".to_string(),
        status: kynex_tray::ItemStatus::Active,
        icon_name: None,
        icon_theme_path: None,
        icon_pixmap: Some(kynex_tray::ValidatedPixmap {
            width: 1,
            height: 1,
            rgba_data: vec![0, 0, 0, 255],
        }),
        tooltip_title: String::new(),
        tooltip_text: String::new(),
        is_menu: false,
        window_id: 0,
        attention_icon_name: None,
        attention_pixmap: Some(kynex_tray::ValidatedPixmap {
            width: 1,
            height: 1,
            rgba_data: vec![255, 0, 0, 255],
        }),
    });
    assert_eq!(info.icon_pixmap_width, 1);
    assert_eq!(info.attention_pixmap_width, 1);

    Ok(())
}

#[tokio::test]
async fn test_malformed_ids_and_paths() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    // Malformed IDs to parse_item_id
    assert!(tray1.activate("/path_only_no_bus", 0, 0).await.is_err());
    assert!(tray1
        .secondary_activate("/path_only_no_bus", 0, 0)
        .await
        .is_err());
    assert!(tray1.context_menu("/path_only_no_bus", 0, 0).await.is_err());
    assert!(tray1
        .scroll("/path_only_no_bus", 0, "vertical")
        .await
        .is_err());

    // Nonexistent item key
    assert!(tray1.activate(":1.999/nonexistent", 0, 0).await.is_err());
    assert!(tray1
        .secondary_activate(":1.999/nonexistent", 0, 0)
        .await
        .is_err());
    assert!(tray1
        .context_menu(":1.999/nonexistent", 0, 0)
        .await
        .is_err());
    assert!(tray1
        .scroll(":1.999/nonexistent", 0, "vertical")
        .await
        .is_err());

    // Invalid object path
    assert!(watcher
        .register_status_notifier_item("/invalid..//path")
        .await
        .is_err());

    // Unowned well-known name
    assert!(watcher
        .register_status_notifier_item("com.example.unowned.Service")
        .await
        .is_err());

    // Invalid well-known name
    assert!(watcher
        .register_status_notifier_item("invalid name with spaces/")
        .await
        .is_err());

    Ok(())
}

#[tokio::test]
async fn test_watcher_name_acquisition_and_default_state() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;

    // Test try_acquire_watcher_names when daemon already acquired both names
    let (kde, fdo) =
        kynex_daemon::services::tray::try_acquire_watcher_names(&harness.client_a_conn).await;
    // Names are already owned by daemon connection, so client_a cannot acquire without queuing
    assert!(!kde);
    assert!(!fdo);

    // Test TraySharedState::default()
    let def_state = kynex_daemon::services::tray::TraySharedState::default();
    assert!(!def_state.registry.read().await.is_host_registered());

    Ok(())
}

#[tokio::test]
async fn test_register_by_own_well_known_name_ok() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    let wk_name = "org.kde.StatusNotifierItem.TestAppA";
    harness
        .client_a_conn
        .request_name_with_flags(wk_name, zbus::fdo::RequestNameFlags::DO_NOT_QUEUE)
        .await?;

    let item = FakeItem::default();
    *item.title.write().await = "App A".to_string();
    *item.status.write().await = "Active".to_string();
    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await?;

    watcher.register_status_notifier_item(wk_name).await?;

    let items = watcher.registered_status_notifier_items().await?;
    assert_eq!(items.len(), 1);
    assert_eq!(tray1.get_items().await?.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_register_by_another_clients_well_known_name_rejected() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let wk_name = "org.kde.StatusNotifierItem.TestAppA2";
    harness
        .client_a_conn
        .request_name_with_flags(wk_name, zbus::fdo::RequestNameFlags::DO_NOT_QUEUE)
        .await?;

    let client_b_watcher = StatusNotifierWatcherProxy::builder(&harness.client_b_conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;

    let res = client_b_watcher
        .register_status_notifier_item(wk_name)
        .await;
    assert!(
        res.is_err(),
        "Registering another client's well-known name must be rejected"
    );

    Ok(())
}

#[tokio::test]
async fn test_register_by_unowned_well_known_name_rejected() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;

    let res = watcher
        .register_status_notifier_item("org.kde.StatusNotifierItem.UnownedServiceXYZ")
        .await;
    assert!(
        res.is_err(),
        "Registering unowned well-known name must be rejected"
    );

    Ok(())
}

#[tokio::test]
async fn test_register_by_object_path_ok() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;

    let item = FakeItem::default();
    *item.title.write().await = "Path Only".to_string();
    *item.status.write().await = "Active".to_string();
    harness
        .client_a_conn
        .object_server()
        .at("/CustomTrayPath", item)
        .await?;

    watcher
        .register_status_notifier_item("/CustomTrayPath")
        .await?;

    let items = watcher.registered_status_notifier_items().await?;
    assert_eq!(items.len(), 1);
    assert!(items[0].ends_with("/CustomTrayPath"));

    Ok(())
}

#[tokio::test]
async fn test_register_by_own_unique_name_ok() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;

    let item = FakeItem::default();
    *item.title.write().await = "Unique Name".to_string();
    *item.status.write().await = "Active".to_string();
    harness
        .client_a_conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await?;

    let my_unique = harness.client_a_conn.unique_name().unwrap().as_str();
    watcher.register_status_notifier_item(my_unique).await?;

    let items = watcher.registered_status_notifier_items().await?;
    assert_eq!(items.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_register_well_known_name_owner_changes_mid_way() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;

    let dedicated_conn = zbus::connection::Builder::address(harness._bus.address())
        .build()
        .await?;
    let wk_name = "org.kde.StatusNotifierItem.TransientOwnerApp";
    dedicated_conn
        .request_name_with_flags(wk_name, zbus::fdo::RequestNameFlags::DO_NOT_QUEUE)
        .await?;

    let item = FakeItem::default();
    *item.title.write().await = "Transient App".to_string();
    *item.status.write().await = "Active".to_string();
    dedicated_conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await?;

    let dedicated_watcher = StatusNotifierWatcherProxy::builder(&dedicated_conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;

    dedicated_watcher
        .register_status_notifier_item(wk_name)
        .await?;

    assert_eq!(watcher.registered_status_notifier_items().await?.len(), 1);

    // Release well-known name mid-way
    dedicated_conn.release_name(wk_name).await?;

    // Wait for NameOwnerChanged to unregister item
    let mut cleared = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if watcher.registered_status_notifier_items().await?.is_empty() {
            cleared = true;
            break;
        }
    }
    assert!(
        cleared,
        "Item must be automatically unregistered when well-known name owner changes"
    );

    Ok(())
}

#[tokio::test]
async fn test_signal_flood_decoupled_resilience() -> Result<(), Box<dyn Error>> {
    use std::time::Instant;

    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;
    let tray1 = harness.tray1_client().await?;

    // 1. Export Fast Item on client_a_conn
    let fast_item = FakeItem::default();
    *fast_item.title.write().await = "Fast Item Initial".to_string();
    *fast_item.status.write().await = "Active".to_string();
    *fast_item.icon_name.write().await = "fast-icon-init".to_string();
    harness
        .client_a_conn
        .object_server()
        .at("/FastItem", fast_item.clone())
        .await?;
    watcher.register_status_notifier_item("/FastItem").await?;

    // 2. Export Slow Answering Item on client_b_conn (1 second delay on property fetch)
    let slow_item = FakeItem::default();
    *slow_item.title.write().await = "Slow Item Initial".to_string();
    *slow_item.status.write().await = "Active".to_string();
    *slow_item.icon_name.write().await = "slow-icon-init".to_string();
    *slow_item.delay_ms.write().await = 1000;
    harness
        .client_b_conn
        .object_server()
        .at("/SlowItem", slow_item.clone())
        .await?;

    let client_b_watcher = StatusNotifierWatcherProxy::builder(&harness.client_b_conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    client_b_watcher
        .register_status_notifier_item("/SlowItem")
        .await?;

    // Trigger Slow Item refresh so it begins a 1s property fetch
    let slow_emitter = SignalEmitter::new(&harness.client_b_conn, "/SlowItem")?;
    let _ = FakeItem::new_icon(&slow_emitter).await;

    // 3. Flood Fast Item with 10,000 NewIcon signals in background
    let fast_conn = harness.client_a_conn.clone();
    let fast_emitter = SignalEmitter::new(&fast_conn, "/FastItem")?;
    let flood_handle = tokio::spawn(async move {
        for _ in 0..10_000 {
            let _ = FakeItem::new_icon(&fast_emitter).await;
        }
    });

    // 4. Meanwhile, Client C connects, calls Core1.Ping() and registers a new item
    let client_c_conn = zbus::connection::Builder::address(harness._bus.address())
        .build()
        .await?;
    let client_c_core1 = kynex_ipc::Core1Proxy::builder(&client_c_conn)
        .destination(DAEMON_DESTINATION)
        .path(kynex_ipc::DAEMON_PATH)
        .build()
        .await?;
    let client_c_watcher = StatusNotifierWatcherProxy::builder(&client_c_conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;

    // Ping check: must finish under 100 ms
    let harness_core1 = harness.core1_client().await?;
    let _ = harness_core1.ping().await?;

    let t_ping = Instant::now();
    let pong = client_c_core1.ping().await?;
    let ping_duration = t_ping.elapsed();
    assert_eq!(pong, "pong");
    assert!(
        ping_duration < Duration::from_millis(100),
        "Core1.Ping took {ping_duration:?}, exceeding 100 ms during signal flood"
    );

    // Register new item check: must finish under 100 ms
    let new_item_c = FakeItem::default();
    *new_item_c.title.write().await = "New Item C".to_string();
    *new_item_c.status.write().await = "Active".to_string();
    client_c_conn
        .object_server()
        .at("/NewItemC", new_item_c)
        .await?;

    let t_reg = Instant::now();
    client_c_watcher
        .register_status_notifier_item("/NewItemC")
        .await?;
    let reg_duration = t_reg.elapsed();
    assert!(
        reg_duration < Duration::from_millis(100),
        "Registering item took {reg_duration:?}, exceeding 100 ms during signal flood"
    );

    // Wait for signal emission to complete
    flood_handle.await?;

    // Set final states on fast and slow items
    *fast_item.title.write().await = "Fast Item Final".to_string();
    *fast_item.icon_name.write().await = "fast-icon-final".to_string();
    let fast_emitter_final = SignalEmitter::new(&harness.client_a_conn, "/FastItem")?;
    let _ = FakeItem::new_icon(&fast_emitter_final).await;

    *slow_item.delay_ms.write().await = 0;
    *slow_item.title.write().await = "Slow Item Final".to_string();
    *slow_item.icon_name.write().await = "slow-icon-final".to_string();
    let slow_emitter_final = SignalEmitter::new(&harness.client_b_conn, "/SlowItem")?;
    let _ = FakeItem::new_icon(&slow_emitter_final).await;

    // Wait for debounced trailing fetches to settle
    let mut final_state_correct = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let items = tray1.get_items().await?;
        if items.len() == 3 {
            let fast_ok = items
                .iter()
                .any(|i| i.title == "Fast Item Final" && i.icon_name == "fast-icon-final");
            let slow_ok = items
                .iter()
                .any(|i| i.title == "Slow Item Final" && i.icon_name == "slow-icon-final");
            let new_c_ok = items.iter().any(|i| i.title == "New Item C");
            if fast_ok && slow_ok && new_c_ok {
                final_state_correct = true;
                break;
            }
        }
    }

    assert!(
        final_state_correct,
        "Final state of all items must be correct after signal flood and slow item completion"
    );

    Ok(())
}

#[tokio::test]
async fn test_multi_client_65_items_global_cap_enforced() -> Result<(), Box<dyn Error>> {
    let harness = TestHarness::new().await?;
    let watcher = harness.kde_watcher_client().await?;

    // Create 16 distinct client connections registering 4 items each = 64 items total
    let mut client_conns = Vec::new();
    for s in 0..16 {
        let conn = zbus::connection::Builder::address(harness._bus.address())
            .build()
            .await?;
        for i in 1..=4 {
            let path = format!("/Item{i}");
            let item = FakeItem::default();
            *item.title.write().await = format!("Client {s} Item {i}");
            *item.status.write().await = "Active".to_string();
            conn.object_server().at(path.as_str(), item).await?;

            let client_watcher = StatusNotifierWatcherProxy::builder(&conn)
                .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
                .path(STATUS_NOTIFIER_WATCHER_PATH)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await?;
            client_watcher
                .register_status_notifier_item(path.as_str())
                .await?;
        }
        client_conns.push(conn);
    }

    let items = watcher.registered_status_notifier_items().await?;
    assert_eq!(
        items.len(),
        64,
        "Expected exactly 64 items registered across 16 clients"
    );

    // 17th client connection attempts to register the 65th item
    let client_17 = zbus::connection::Builder::address(harness._bus.address())
        .build()
        .await?;
    let item65 = FakeItem::default();
    *item65.title.write().await = "Item 65".to_string();
    *item65.status.write().await = "Active".to_string();
    client_17.object_server().at("/Item65", item65).await?;

    let watcher_17 = StatusNotifierWatcherProxy::builder(&client_17)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;

    let res65 = watcher_17.register_status_notifier_item("/Item65").await;
    assert!(
        res65.is_err(),
        "The 65th item must be rejected by total capacity limit (max 64)"
    );

    // Also verify per-sender cap: Client 0 attempts to register a 5th item
    let client_0 = &client_conns[0];
    let item_extra = FakeItem::default();
    *item_extra.title.write().await = "Client 0 Item 5".to_string();
    *item_extra.status.write().await = "Active".to_string();
    client_0.object_server().at("/Item5", item_extra).await?;

    let watcher_0 = StatusNotifierWatcherProxy::builder(client_0)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;

    let res_extra = watcher_0.register_status_notifier_item("/Item5").await;
    assert!(
        res_extra.is_err(),
        "5th item from a single sender must be rejected (max 4 per sender)"
    );

    // Total count remains 64
    assert_eq!(watcher.registered_status_notifier_items().await?.len(), 64);

    Ok(())
}
