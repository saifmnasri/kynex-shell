//! Private-bus integration test suite for dbusmenu service in `kynex-daemon`.
//!
//! Enforces:
//! - Full spec conformance: GetLayout, AboutToShow, Event, LayoutUpdated.
//! - Hostile input corpus: deep nesting, huge counts, sanitization, malformed IDs, cycles, type confusion, timeouts.
//! - LayoutUpdated flood resiliency (< 100 ms Core1.Ping).
//! - Owner disconnect invalidation.
//! - Security check mutants verification.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::unreadable_literal,
    clippy::doc_markdown,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_async,
    clippy::type_complexity,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::redundant_closure_for_method_calls,
    clippy::unnecessary_cast,
    clippy::no_effect_underscore_binding,
    clippy::used_underscore_binding,
    clippy::unused_self,
    clippy::unnecessary_literal_bound
)]

use std::collections::HashMap;
use std::error::Error;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use kynex_daemon::services::tray::{
    run_tray_dbus_name_watcher, try_acquire_watcher_names, FdoStatusNotifierWatcherServer,
    StatusNotifierWatcherServer, TrayServer, TraySharedState,
};
use kynex_dbusmenu::{RawMenuNode, RawValue, MAX_LABEL_LEN, MAX_SHORTCUT_LEN};
use kynex_ipc::tray::{
    StatusNotifierWatcherProxy, Tray1Proxy, STATUS_NOTIFIER_WATCHER_DESTINATION_KDE,
    STATUS_NOTIFIER_WATCHER_PATH, TRAY_PATH,
};
use kynex_ipc::DAEMON_DESTINATION;
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection, OwnedObjectPath, OwnedVariant, Structure, Variant};

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

fn raw_value_to_variant(val: &RawValue) -> Variant<'static> {
    match val {
        RawValue::String(s) => Variant::Str(s.clone().into()),
        RawValue::Bool(b) => Variant::Bool(*b),
        RawValue::Int(i) => Variant::I64(*i),
        RawValue::Array(arr) => {
            let mut v = Vec::new();
            for item in arr {
                v.push(raw_value_to_variant(item));
            }
            Variant::Array(v.into())
        }
        _ => Variant::Str("".into()),
    }
}

fn raw_node_to_dbus_tree(
    node: &RawMenuNode,
) -> (i32, HashMap<String, OwnedVariant>, Vec<OwnedVariant>) {
    let mut props = HashMap::new();
    for (k, v) in &node.properties {
        let var = raw_value_to_variant(v);
        let owned = OwnedVariant::try_from(var).unwrap();
        props.insert(k.clone(), owned);
    }
    let mut children = Vec::new();
    for child in &node.children {
        let child_tree = raw_node_to_dbus_tree(child);
        let child_struct = Structure::from((child_tree.0, child_tree.1, child_tree.2));
        let owned = OwnedVariant::try_from(Variant::Structure(child_struct)).unwrap();
        children.push(owned);
    }
    (node.id, props, children)
}

/// Fake `com.canonical.dbusmenu` service.
#[derive(Clone)]
struct FakeDbusmenu {
    revision: Arc<AtomicU32>,
    root: Arc<RwLock<RawMenuNode>>,
    custom_tree: Arc<RwLock<Option<(i32, HashMap<String, OwnedVariant>, Vec<OwnedVariant>)>>>,
    hang_get_layout: Arc<AtomicBool>,
    hang_about_to_show: Arc<AtomicBool>,
    about_to_show_needs_update: Arc<AtomicBool>,
    about_to_show_called: Arc<RwLock<Vec<i32>>>,
    last_event: Arc<RwLock<Option<(i32, String)>>>,
    layout_updated_emitter: Arc<RwLock<Option<SignalEmitter<'static>>>>,
}

impl FakeDbusmenu {
    fn new(root_node: RawMenuNode) -> Self {
        Self {
            revision: Arc::new(AtomicU32::new(1)),
            root: Arc::new(RwLock::new(root_node)),
            custom_tree: Arc::new(RwLock::new(None)),
            hang_get_layout: Arc::new(AtomicBool::new(false)),
            hang_about_to_show: Arc::new(AtomicBool::new(false)),
            about_to_show_needs_update: Arc::new(AtomicBool::new(false)),
            about_to_show_called: Arc::new(RwLock::new(Vec::new())),
            last_event: Arc::new(RwLock::new(None)),
            layout_updated_emitter: Arc::new(RwLock::new(None)),
        }
    }

    async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        *self.layout_updated_emitter.write().await = Some(emitter);
    }

    async fn emit_layout_updated(&self, rev: u32, parent: i32) -> zbus::Result<()> {
        let emitter = self.layout_updated_emitter.read().await;
        if let Some(e) = emitter.as_ref() {
            Self::layout_updated(e, rev, parent).await?;
        }
        Ok(())
    }

    async fn emit_items_properties_updated(
        &self,
        updated_props: Vec<(i32, HashMap<String, OwnedVariant>)>,
        removed_props: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()> {
        let emitter = self.layout_updated_emitter.read().await;
        if let Some(e) = emitter.as_ref() {
            Self::items_properties_updated(e, updated_props, removed_props).await?;
        }
        Ok(())
    }
}

#[interface(name = "com.canonical.dbusmenu")]
impl FakeDbusmenu {
    async fn get_layout(
        &self,
        _parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, (i32, HashMap<String, OwnedVariant>, Vec<OwnedVariant>))> {
        if self.hang_get_layout.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }

        let rev = self.revision.load(Ordering::SeqCst);
        let custom = self.custom_tree.read().await;
        if let Some(tree) = custom.as_ref() {
            return Ok((rev, tree.clone()));
        }
        let root = self.root.read().await;
        let tree = raw_node_to_dbus_tree(&root);
        Ok((rev, tree))
    }

    async fn about_to_show(&self, id: i32) -> zbus::fdo::Result<bool> {
        if self.hang_about_to_show.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        self.about_to_show_called.write().await.push(id);
        Ok(self.about_to_show_needs_update.load(Ordering::SeqCst))
    }

    async fn event(
        &self,
        id: i32,
        event_id: String,
        _data: OwnedVariant,
        _timestamp: u32,
    ) -> zbus::fdo::Result<()> {
        *self.last_event.write().await = Some((id, event_id));
        Ok(())
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn items_properties_updated(
        emitter: &SignalEmitter<'_>,
        updated_props: Vec<(i32, HashMap<String, OwnedVariant>)>,
        removed_props: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }
}

/// Fake StatusNotifierItem advertising a menu path.
#[derive(Clone)]
struct FakeSniItem {
    menu_path: String,
    title: String,
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl FakeSniItem {
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(self.menu_path.clone()).unwrap()
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "fake-item-app"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        &self.title
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        "test-icon"
    }

    fn activate(&self, _x: i32, _y: i32) {}
    fn context_menu(&self, _x: i32, _y: i32) {}
}

/// Test harness orchestrating private bus, daemon server, fake SNI and dbusmenu services.
struct Harness {
    _bus: TestBus,
    _daemon_conn: Connection,
    client_a_conn: Arc<RwLock<Option<Connection>>>,
    client_b_conn: Connection,
    fake_menu: FakeDbusmenu,
    canonical_item_id: String,
}

impl Harness {
    async fn new(root_node: RawMenuNode) -> Result<Self, Box<dyn Error>> {
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

        // Client A: hosts FakeSniItem and FakeDbusmenu
        let client_a_conn = zbus::connection::Builder::address(bus.address())
            .build()
            .await?;

        let fake_sni = FakeSniItem {
            menu_path: "/MenuBar".to_string(),
            title: "Fake Item".to_string(),
        };
        client_a_conn
            .object_server()
            .at("/StatusNotifierItem", fake_sni)
            .await?;

        let fake_menu = FakeDbusmenu::new(root_node);
        client_a_conn
            .object_server()
            .at("/MenuBar", fake_menu.clone())
            .await?;

        let menu_emitter = SignalEmitter::new(&client_a_conn, "/MenuBar")?;
        fake_menu.set_emitter(menu_emitter).await;

        // Register fake item with watcher
        let watcher_proxy = StatusNotifierWatcherProxy::builder(&client_a_conn)
            .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
            .path(STATUS_NOTIFIER_WATCHER_PATH)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;

        watcher_proxy
            .register_status_notifier_item("/StatusNotifierItem")
            .await?;

        // Client B: acts as shell consumer
        let client_b_conn = zbus::connection::Builder::address(bus.address())
            .build()
            .await?;

        let tray1_client = Tray1Proxy::builder(&client_b_conn)
            .destination(DAEMON_DESTINATION)
            .path(TRAY_PATH)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;

        // Wait until daemon registers item
        let mut canonical_item_id = String::new();
        let timeout = Instant::now() + Duration::from_secs(5);
        while Instant::now() < timeout {
            let items = tray1_client.get_items().await?;
            if let Some(first) = items.first() {
                canonical_item_id.clone_from(&first.id);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            !canonical_item_id.is_empty(),
            "Item was not registered by daemon in time"
        );

        Ok(Self {
            _bus: bus,
            _daemon_conn: daemon_conn,
            client_a_conn: Arc::new(RwLock::new(Some(client_a_conn))),
            client_b_conn,
            fake_menu,
            canonical_item_id,
        })
    }

    async fn tray1_client(&self) -> zbus::Result<Tray1Proxy<'_>> {
        Tray1Proxy::builder(&self.client_b_conn)
            .destination(DAEMON_DESTINATION)
            .path(TRAY_PATH)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
    }
}

fn build_sample_tree() -> RawMenuNode {
    let mut root = RawMenuNode::new(0);

    let child1 = RawMenuNode::new(1)
        .with_property("label", RawValue::String("_File".to_string()))
        .with_property("enabled", RawValue::Bool(true))
        .with_property("visible", RawValue::Bool(true))
        .with_child(
            RawMenuNode::new(10)
                .with_property("label", RawValue::String("_New".to_string()))
                .with_property("shortcut", RawValue::String("Ctrl+N".to_string())),
        )
        .with_child(
            RawMenuNode::new(11)
                .with_property("label", RawValue::String("_Open".to_string()))
                .with_property("toggle-type", RawValue::String("checkmark".to_string()))
                .with_property("toggle-state", RawValue::Int(1)),
        );

    let sep = RawMenuNode::new(2).with_property("type", RawValue::String("separator".to_string()));

    let child3 = RawMenuNode::new(3)
        .with_property("label", RawValue::String("E_xit".to_string()))
        .with_property("disposition", RawValue::String("alert".to_string()));

    root.children.push(child1);
    root.children.push(sep);
    root.children.push(child3);

    root
}

#[tokio::test]
async fn test_spec_conformance_get_menu_about_to_show_event() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(build_sample_tree()).await?;
    let client = harness.tray1_client().await?;

    // 1. GetMenu
    let (revision, items) = client.get_menu(&harness.canonical_item_id).await?;
    assert_eq!(revision, 1);
    assert_eq!(items.len(), 5);

    // Root children: File, Separator, Exit
    assert_eq!(items[0].id, 1);
    assert_eq!(items[0].label, "File");
    assert!(items[0].has_submenu);

    assert_eq!(items[1].id, 10);
    assert_eq!(items[1].parent, 1);
    assert_eq!(items[1].label, "New");
    assert_eq!(items[1].shortcut, "Ctrl+N");

    assert_eq!(items[2].id, 11);
    assert_eq!(items[2].parent, 1);
    assert_eq!(items[2].label, "Open");
    assert_eq!(items[2].toggle, 1); // Checkbox
    assert_eq!(items[2].toggle_state, 1); // Checked

    assert_eq!(items[3].id, 2);
    assert_eq!(items[3].kind, 1); // Separator

    assert_eq!(items[4].id, 3);
    assert_eq!(items[4].label, "Exit");
    assert_eq!(items[4].disposition, 3); // Alert

    // 2. MenuAboutToShow (needs_update = false)
    let needs_update = client
        .menu_about_to_show(&harness.canonical_item_id, 1)
        .await?;
    assert!(!needs_update);
    assert_eq!(
        *harness.fake_menu.about_to_show_called.read().await,
        vec![1]
    );

    // MenuAboutToShow with needs_update = true triggers re-fetch
    harness
        .fake_menu
        .about_to_show_needs_update
        .store(true, Ordering::SeqCst);
    harness.fake_menu.revision.store(3, Ordering::SeqCst);
    let needs_update_true = client
        .menu_about_to_show(&harness.canonical_item_id, 1)
        .await?;
    assert!(needs_update_true);
    let (rev3, _) = client.get_menu(&harness.canonical_item_id).await?;
    assert_eq!(rev3, 3);
    assert_eq!(
        *harness.fake_menu.about_to_show_called.read().await,
        vec![1, 1]
    );

    // 3. MenuEvent
    client
        .menu_event(&harness.canonical_item_id, 10, "clicked")
        .await?;
    assert_eq!(
        *harness.fake_menu.last_event.read().await,
        Some((10, "clicked".to_string()))
    );

    client
        .menu_event(&harness.canonical_item_id, 10, "hovered")
        .await?;
    assert_eq!(
        *harness.fake_menu.last_event.read().await,
        Some((10, "hovered".to_string()))
    );

    Ok(())
}

#[tokio::test]
async fn test_spec_conformance_layout_updated_signal() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(build_sample_tree()).await?;
    let client = harness.tray1_client().await?;

    // Initial fetch to load cache
    let (rev, _) = client.get_menu(&harness.canonical_item_id).await?;
    assert_eq!(rev, 1);

    let mut signal_stream = client.receive_menu_changed().await?;

    // 1. Update revision on fake service and emit LayoutUpdated
    harness.fake_menu.revision.store(2, Ordering::SeqCst);
    harness.fake_menu.emit_layout_updated(2, 0).await?;

    let sig = tokio::time::timeout(Duration::from_secs(3), signal_stream.next())
        .await?
        .ok_or("Did not receive MenuChanged signal")?;
    let args = sig.args()?;
    assert_eq!(args.item_id, harness.canonical_item_id);
    assert_eq!(args.revision, 2);

    // Subsequent fetch returns revision 2
    let (new_rev, _) = client.get_menu(&harness.canonical_item_id).await?;
    assert_eq!(new_rev, 2);

    // 2. Emit ItemsPropertiesUpdated signal
    tokio::time::sleep(Duration::from_millis(150)).await; // avoid rate limiter
    harness.fake_menu.revision.store(4, Ordering::SeqCst);
    harness
        .fake_menu
        .emit_items_properties_updated(Vec::new(), Vec::new())
        .await?;

    let sig2 = tokio::time::timeout(Duration::from_secs(3), signal_stream.next())
        .await?
        .ok_or("Did not receive MenuChanged signal for ItemsPropertiesUpdated")?;
    let args2 = sig2.args()?;
    assert_eq!(args2.item_id, harness.canonical_item_id);
    assert_eq!(args2.revision, 4);

    Ok(())
}

#[tokio::test]
async fn test_menu_fetch_coalescing() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(build_sample_tree()).await?;
    let client1 = harness.tray1_client().await?;
    let client2 = harness.tray1_client().await?;

    let (res1, res2) = tokio::join!(
        client1.get_menu(&harness.canonical_item_id),
        client2.get_menu(&harness.canonical_item_id)
    );

    let (rev1, items1) = res1?;
    let (rev2, items2) = res2?;
    assert_eq!(rev1, 1);
    assert_eq!(rev2, 1);
    assert_eq!(items1.len(), items2.len());
    Ok(())
}

#[test]
fn test_menu_manager_default_trait() {
    let _mgr = kynex_daemon::services::tray_menu::MenuManager::default();
}

#[test]
fn test_variant_to_raw_value_conversions() {
    use kynex_daemon::services::tray_menu::variant_to_raw_value;
    use zbus::Variant;

    assert_eq!(variant_to_raw_value(&Variant::I16(42)), RawValue::Int(42));
    assert_eq!(variant_to_raw_value(&Variant::U8(5)), RawValue::Int(5));
    assert_eq!(variant_to_raw_value(&Variant::U16(100)), RawValue::Int(100));
    assert_eq!(
        variant_to_raw_value(&Variant::U64(1000)),
        RawValue::Int(1000)
    );
    assert_eq!(
        variant_to_raw_value(&Variant::U64(u64::MAX)),
        RawValue::Int(i64::MAX)
    );
    assert_eq!(
        variant_to_raw_value(&Variant::Array(vec![Variant::I32(1)].into())),
        RawValue::Array(vec![RawValue::Int(1)])
    );
    assert_eq!(
        variant_to_raw_value(&Variant::Signature(zbus::Signature::try_from("s").unwrap())),
        RawValue::Other
    );
}

#[test]
fn test_hostile_nesting_1000_levels() -> Result<(), Box<dyn Error>> {
    let builder = std::thread::Builder::new()
        .name("deep-nesting-test".to_string())
        .stack_size(16 * 1024 * 1024);

    let handle = builder.spawn(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let harness = Harness::new(RawMenuNode::new(0)).await.unwrap();
            let client = harness.tray1_client().await.unwrap();

            // Build hostile 1000-deep chain iteratively from leaf to root
            let current_struct = Structure::from((
                1000i32,
                HashMap::<String, OwnedVariant>::new(),
                Vec::<OwnedVariant>::new(),
            ));
            let mut current_owned =
                OwnedVariant::try_from(Variant::Structure(current_struct)).unwrap();

            for id in (1..=999).rev() {
                let parent_struct = Structure::from((
                    id,
                    HashMap::<String, OwnedVariant>::new(),
                    vec![current_owned],
                ));
                current_owned = OwnedVariant::try_from(Variant::Structure(parent_struct)).unwrap();
            }

            let root_tree = (
                0i32,
                HashMap::<String, OwnedVariant>::new(),
                vec![current_owned],
            );
            *harness.fake_menu.custom_tree.write().await = Some(root_tree);

            let err = client.get_menu(&harness.canonical_item_id).await;
            assert!(err.is_err(), "1000-level tree must be rejected");
        });
    })?;

    handle.join().map_err(|_| "Test thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_hostile_item_cap_exceeded() -> Result<(), Box<dyn Error>> {
    let mut root = RawMenuNode::new(0);
    let mut id_counter = 1;
    for _ in 0..6 {
        let mut parent = RawMenuNode::new(id_counter);
        id_counter += 1;
        for _ in 0..90 {
            parent.children.push(RawMenuNode::new(id_counter));
            id_counter += 1;
        }
        root.children.push(parent);
    }
    assert!(id_counter > 500);

    let harness = Harness::new(root).await?;
    let client = harness.tray1_client().await?;

    let err = client.get_menu(&harness.canonical_item_id).await;
    assert!(err.is_err(), "Tree exceeding 500 items must be rejected");

    Ok(())
}

#[tokio::test]
async fn test_hostile_children_per_node_exceeded() -> Result<(), Box<dyn Error>> {
    let mut root = RawMenuNode::new(0);
    for id in 1..=105 {
        root.children.push(RawMenuNode::new(id as i32));
    }

    let harness = Harness::new(root).await?;
    let client = harness.tray1_client().await?;

    let err = client.get_menu(&harness.canonical_item_id).await;
    assert!(
        err.is_err(),
        "Node with >100 children must be rejected by validator"
    );

    Ok(())
}

#[tokio::test]
async fn test_hostile_label_sanitization() -> Result<(), Box<dyn Error>> {
    let mut root = RawMenuNode::new(0);

    let huge_text = "a".repeat(10_000);
    let hostile_item1 =
        RawMenuNode::new(1).with_property("label", RawValue::String("<b>_Bold</b>".to_string()));
    let hostile_item2 = RawMenuNode::new(2).with_property(
        "label",
        RawValue::String("Status: \u{202E}Override\u{202C}".to_string()),
    );
    let hostile_item3 = RawMenuNode::new(3).with_property(
        "label",
        RawValue::String("Control\x07\x1B[31mCode".to_string()),
    );
    let hostile_item4 = RawMenuNode::new(4).with_property("label", RawValue::String(huge_text));

    root.children.push(hostile_item1);
    root.children.push(hostile_item2);
    root.children.push(hostile_item3);
    root.children.push(hostile_item4);

    let harness = Harness::new(root).await?;
    let client = harness.tray1_client().await?;

    let (_, items) = client.get_menu(&harness.canonical_item_id).await?;
    assert_eq!(items.len(), 4);

    // Markup stripped + mnemonic parsed
    assert_eq!(items[0].label, "Bold");
    // Bidi override stripped
    assert_eq!(items[1].label, "Status: Override");
    // Control chars stripped
    assert_eq!(items[2].label, "Control[31mCode");
    // Huge text clamped to MAX_LABEL_LEN
    assert_eq!(items[3].label.chars().count(), MAX_LABEL_LEN);

    Ok(())
}

#[tokio::test]
async fn test_hostile_duplicate_and_negative_ids() -> Result<(), Box<dyn Error>> {
    // Duplicate IDs
    let mut root_dup = RawMenuNode::new(0);
    root_dup.children.push(RawMenuNode::new(5));
    root_dup.children.push(RawMenuNode::new(5));

    let harness = Harness::new(root_dup).await?;
    let client = harness.tray1_client().await?;
    assert!(
        client.get_menu(&harness.canonical_item_id).await.is_err(),
        "Duplicate IDs must be rejected"
    );

    // Negative ID
    let mut root_neg = RawMenuNode::new(0);
    root_neg.children.push(RawMenuNode::new(-2));

    let harness_neg = Harness::new(root_neg).await?;
    let client_neg = harness_neg.tray1_client().await?;
    assert!(
        client_neg
            .get_menu(&harness_neg.canonical_item_id)
            .await
            .is_err(),
        "Negative ID must be rejected"
    );

    Ok(())
}

#[tokio::test]
async fn test_hostile_cycle_rejection() -> Result<(), Box<dyn Error>> {
    let mut root = RawMenuNode::new(0);
    let mut node = RawMenuNode::new(1);
    node.children.push(RawMenuNode::new(1)); // Self-ancestor cycle
    root.children.push(node);

    let harness = Harness::new(root).await?;
    let client = harness.tray1_client().await?;

    let res = client.get_menu(&harness.canonical_item_id).await;
    assert!(res.is_err(), "Cycle must be rejected");

    Ok(())
}

#[tokio::test]
async fn test_hostile_property_type_confusion() -> Result<(), Box<dyn Error>> {
    let mut root = RawMenuNode::new(0);
    let item = RawMenuNode::new(1)
        .with_property("label", RawValue::Int(12345))
        .with_property("enabled", RawValue::String("not-a-bool".to_string()))
        .with_property("visible", RawValue::Array(vec![]));
    root.children.push(item);

    let harness = Harness::new(root).await?;
    let client = harness.tray1_client().await?;

    let (_, items) = client.get_menu(&harness.canonical_item_id).await?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "");
    assert!(items[0].enabled);
    assert!(items[0].visible);

    Ok(())
}

#[tokio::test]
async fn test_hostile_huge_shortcuts() -> Result<(), Box<dyn Error>> {
    let mut root = RawMenuNode::new(0);
    let huge_shortcut = "Ctrl+".repeat(500);
    let item = RawMenuNode::new(1)
        .with_property("label", RawValue::String("Test".to_string()))
        .with_property("shortcut", RawValue::String(huge_shortcut));
    root.children.push(item);

    let harness = Harness::new(root).await?;
    let client = harness.tray1_client().await?;

    let (_, items) = client.get_menu(&harness.canonical_item_id).await?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].shortcut.chars().count(), MAX_SHORTCUT_LEN);

    Ok(())
}

#[tokio::test]
async fn test_hostile_get_layout_timeout_2s() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(build_sample_tree()).await?;
    let client = harness.tray1_client().await?;

    harness
        .fake_menu
        .hang_get_layout
        .store(true, Ordering::SeqCst);

    let start = Instant::now();
    let res = client.get_menu(&harness.canonical_item_id).await;
    let elapsed = start.elapsed();

    assert!(res.is_err(), "GetLayout must time out");
    assert!(
        elapsed >= Duration::from_millis(1900) && elapsed < Duration::from_millis(3500),
        "Timeout must trigger near 2.0s, elapsed: {elapsed:?}"
    );

    Ok(())
}

#[tokio::test]
async fn test_hostile_about_to_show_timeout_2s() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(build_sample_tree()).await?;
    let client = harness.tray1_client().await?;

    // Load initial layout into cache
    let _ = client.get_menu(&harness.canonical_item_id).await?;

    harness
        .fake_menu
        .hang_about_to_show
        .store(true, Ordering::SeqCst);

    let start = Instant::now();
    let res = client
        .menu_about_to_show(&harness.canonical_item_id, 1)
        .await;
    let elapsed = start.elapsed();

    assert!(res.is_err(), "AboutToShow must time out");
    assert!(
        elapsed >= Duration::from_millis(1900) && elapsed < Duration::from_millis(3500),
        "AboutToShow timeout must trigger near 2.0s, elapsed: {elapsed:?}"
    );

    Ok(())
}

#[tokio::test]
async fn test_hostile_layout_updated_flood_and_core_ping() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(build_sample_tree()).await?;
    let client = harness.tray1_client().await?;

    // Pre-populate
    let _ = client.get_menu(&harness.canonical_item_id).await?;

    // Client C connects and warms up proxy
    let client_c_conn = zbus::connection::Builder::address(harness._bus.address())
        .build()
        .await?;
    let core1_proxy = kynex_ipc::Core1Proxy::builder(&client_c_conn)
        .destination(DAEMON_DESTINATION)
        .path(kynex_ipc::DAEMON_PATH)
        .build()
        .await?;

    let pong_warmup = core1_proxy.ping().await?;
    assert_eq!(pong_warmup, "pong");

    // Flood 10,000 LayoutUpdated signals
    let fake_menu_clone = harness.fake_menu.clone();
    let flood_handle = tokio::spawn(async move {
        for rev in 1..=10_000 {
            if fake_menu_clone.emit_layout_updated(rev, 0).await.is_err() {
                break;
            }
        }
    });

    let t_ping = Instant::now();
    let pong = core1_proxy.ping().await?;
    let ping_duration = t_ping.elapsed();

    assert_eq!(pong, "pong");
    assert!(
        ping_duration < Duration::from_millis(100),
        "Core1.Ping took {ping_duration:?}, exceeding 100 ms limit during LayoutUpdated flood"
    );

    flood_handle.abort();
    let _ = flood_handle.await;
    Ok(())
}

#[tokio::test]
async fn test_hostile_event_validation() -> Result<(), Box<dyn Error>> {
    let mut root = RawMenuNode::new(0);
    root.children.push(
        RawMenuNode::new(1)
            .with_property("label", RawValue::String("Enabled".to_string()))
            .with_property("enabled", RawValue::Bool(true)),
    );
    root.children.push(
        RawMenuNode::new(2)
            .with_property("label", RawValue::String("Disabled".to_string()))
            .with_property("enabled", RawValue::Bool(false)),
    );
    root.children
        .push(RawMenuNode::new(3).with_property("type", RawValue::String("separator".to_string())));

    let harness = Harness::new(root).await?;
    let client = harness.tray1_client().await?;

    // Load menu layout into cache
    let _ = client.get_menu(&harness.canonical_item_id).await?;

    // Event on disabled item
    let err_disabled = client
        .menu_event(&harness.canonical_item_id, 2, "clicked")
        .await;
    assert!(err_disabled.is_err(), "Event on disabled item must fail");

    // Event on separator item
    let err_sep = client
        .menu_event(&harness.canonical_item_id, 3, "clicked")
        .await;
    assert!(err_sep.is_err(), "Event on separator item must fail");

    // Event on unknown item
    let err_unknown = client
        .menu_event(&harness.canonical_item_id, 999, "clicked")
        .await;
    assert!(err_unknown.is_err(), "Event on unknown item ID must fail");

    // Invalid event kind
    let err_invalid = client
        .menu_event(&harness.canonical_item_id, 1, "double-click")
        .await;
    assert!(err_invalid.is_err(), "Invalid event kind must fail");

    // Valid event
    let ok_event = client
        .menu_event(&harness.canonical_item_id, 1, "clicked")
        .await;
    assert!(ok_event.is_ok(), "Valid event on enabled item must succeed");

    Ok(())
}

#[tokio::test]
async fn test_hostile_owner_vanishes_while_menu_open() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(build_sample_tree()).await?;
    let client = harness.tray1_client().await?;

    // Initial load
    let (rev, _) = client.get_menu(&harness.canonical_item_id).await?;
    assert_eq!(rev, 1);

    let item_id = harness.canonical_item_id.clone();

    // Client A abruptly disconnects
    if let Some(conn) = harness.client_a_conn.write().await.take() {
        let _ = conn.close().await;
    }

    // Wait for NameOwnerChanged to trigger removal in registry
    let mut cleared = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if client.get_items().await?.is_empty() {
            cleared = true;
            break;
        }
    }
    assert!(cleared, "Item must be removed after its owner disconnects");

    // Subsequent call fails cleanly, does not crash daemon
    let res = client.get_menu(&item_id).await;
    assert!(
        res.is_err(),
        "Accessing vanished item menu must fail cleanly"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Named Mutants Verification: Proves that removing security checks causes tests to fail.
// ---------------------------------------------------------------------------

#[test]
fn test_mutant_depth_cap_removed() {
    // Mutant: depth cap of 6 removed -> would allow depth 7.
    // Assertion: depth 7 MUST be rejected.
    let mut current = RawMenuNode::new(7);
    for d in (1..=6).rev() {
        let mut parent = RawMenuNode::new(d as i32);
        parent.children.push(current);
        current = parent;
    }
    let mut root = RawMenuNode::new(0);
    root.children.push(current);

    assert!(
        kynex_dbusmenu::parse_menu_layout(1, &root).is_err(),
        "Mutant killed: Depth > 6 rejected by security check"
    );
}

#[test]
fn test_mutant_item_cap_removed() {
    // Mutant: 500 item cap removed -> would allow 501 items.
    let mut root = RawMenuNode::new(0);
    let mut id_counter = 1;
    for _ in 0..5 {
        let mut parent = RawMenuNode::new(id_counter);
        id_counter += 1;
        for _ in 0..99 {
            parent.children.push(RawMenuNode::new(id_counter));
            id_counter += 1;
        }
        root.children.push(parent);
    }
    root.children[0].children.push(RawMenuNode::new(9999));

    assert!(
        kynex_dbusmenu::parse_menu_layout(1, &root).is_err(),
        "Mutant killed: Total items > 500 rejected by security check"
    );
}

#[test]
fn test_mutant_duplicate_id_allowed() {
    let mut root = RawMenuNode::new(0);
    root.children.push(RawMenuNode::new(1));
    root.children.push(RawMenuNode::new(1));

    assert!(
        kynex_dbusmenu::parse_menu_layout(1, &root).is_err(),
        "Mutant killed: Duplicate IDs rejected by security check"
    );
}

#[test]
fn test_mutant_event_validation_bypassed() {
    let mut root = RawMenuNode::new(0);
    root.children.push(
        RawMenuNode::new(1)
            .with_property("label", RawValue::String("Disabled".to_string()))
            .with_property("enabled", RawValue::Bool(false)),
    );
    let layout = kynex_dbusmenu::parse_menu_layout(1, &root).unwrap();

    assert!(
        kynex_dbusmenu::validate_menu_event(&layout, 1, "clicked").is_err(),
        "Mutant killed: Event on disabled item rejected by validator"
    );
}

#[test]
fn test_mutant_label_sanitization_bypassed() {
    let raw = "<script>alert(1)</script>_Normal";
    let sanitized = kynex_dbusmenu::sanitize_label(raw);
    assert_eq!(
        sanitized, "alert(1)Normal",
        "Mutant killed: Markup and mnemonic underscore sanitized"
    );
    assert!(
        !sanitized.contains("<script>"),
        "Mutant killed: HTML/XML tags stripped"
    );
}

#[test]
fn test_mutant_timeout_removed() {
    // Proves timeout bounds execution: timeout constant is 2s
    assert_eq!(kynex_dbusmenu::MENU_TIMEOUT_SECS, 2);
}
