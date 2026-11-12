//! System Tray service implementing `org.kde.StatusNotifierWatcher`,
//! `org.freedesktop.StatusNotifierWatcher`, and `org.kynexos.Daemon.Tray1`.
//!
//! Provides defense-in-depth sanitization, sender verification preventing service spoofing,
//! checked arithmetic pixmap validation, rate-limited refresh updates, non-replacement
//! well-known bus name acquisition, and non-blocking asynchronous item delegation.
#![allow(
    clippy::too_many_arguments,
    clippy::unused_async,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::redundant_closure_for_method_calls,
    clippy::unused_async_trait_impl
)]

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use kynex_ipc::tray::{
    MenuItemDto, StatusNotifierItemProxy, TrayItemInfo, STATUS_NOTIFIER_WATCHER_DESTINATION_FDO,
    STATUS_NOTIFIER_WATCHER_DESTINATION_KDE,
};
use kynex_tray::limits::PROPERTY_TIMEOUT_SECS;
use kynex_tray::pixmap::{select_best_pixmap, RawPixmapTuple};
use kynex_tray::registry::{ItemKey, ItemState, TrayRegistry};
use kynex_tray::status::ItemStatus;
use kynex_tray::{
    sanitize_icon_name, sanitize_id, sanitize_title, sanitize_tooltip_text, sanitize_tooltip_title,
    RefreshRateLimiter, TrayError,
};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{debug, info, warn};
use zbus::fdo::{DBusProxy, PropertiesProxy};
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection, MatchRule, MessageStream};

/// Bounded maximum concurrent property fetch tasks.
const MAX_CONCURRENT_FETCHES: usize = 16;

/// Shared runtime state for the tray subsystem.
pub struct TraySharedState {
    /// Pure Rust item and host registry.
    pub registry: RwLock<TrayRegistry>,
    /// Token-bucket refresh rate limiter.
    pub rate_limiter: Mutex<RefreshRateLimiter>,
    /// D-Bus connection handle for outgoing calls.
    pub conn: RwLock<Option<Connection>>,
    /// Signal emitter for standard `org.kde.StatusNotifierWatcher`.
    pub watcher_emitter: Mutex<Option<SignalEmitter<'static>>>,
    /// Signal emitter for `org.freedesktop.StatusNotifierWatcher`.
    pub fdo_watcher_emitter: Mutex<Option<SignalEmitter<'static>>>,
    /// Signal emitter for internal `org.kynexos.Daemon.Tray1`.
    pub tray1_emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
    /// Whether `org.kde.StatusNotifierWatcher` was successfully acquired on the bus.
    pub kde_name_acquired: AtomicBool,
    /// Whether `org.freedesktop.StatusNotifierWatcher` was successfully acquired on the bus.
    pub fdo_name_acquired: AtomicBool,
    /// Cancellation senders for active item signal listener tasks.
    pub item_cancellers: Mutex<HashMap<ItemKey, tokio::sync::oneshot::Sender<()>>>,
    /// Tracked well-known bus names mapped to their registered item keys.
    pub well_known_owners: Mutex<HashMap<String, Vec<ItemKey>>>,
    /// Semaphore enforcing a bounded cap on concurrent property fetches.
    pub fetch_semaphore: Arc<Semaphore>,
    /// Menu manager handling dbusmenu fetching, caching, rate limiting, and events.
    pub menu_manager: Arc<crate::services::tray_menu::MenuManager>,
}

impl TraySharedState {
    /// Creates a new shared state instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(TrayRegistry::new()),
            rate_limiter: Mutex::new(RefreshRateLimiter::new()),
            conn: RwLock::new(None),
            watcher_emitter: Mutex::new(None),
            fdo_watcher_emitter: Mutex::new(None),
            tray1_emitter: Arc::new(Mutex::new(None)),
            kde_name_acquired: AtomicBool::new(false),
            fdo_name_acquired: AtomicBool::new(false),
            item_cancellers: Mutex::new(HashMap::new()),
            well_known_owners: Mutex::new(HashMap::new()),
            fetch_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES)),
            menu_manager: Arc::new(crate::services::tray_menu::MenuManager::new()),
        }
    }

    /// Stores the active D-Bus connection.
    pub async fn set_connection(&self, conn: Connection) {
        *self.conn.write().await = Some(conn);
    }
}

impl Default for TraySharedState {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard `org.kde.StatusNotifierWatcher` service endpoint.
#[derive(Clone)]
pub struct StatusNotifierWatcherServer {
    shared: Arc<TraySharedState>,
}

impl StatusNotifierWatcherServer {
    /// Creates a new watcher server.
    #[must_use]
    pub fn new(shared: Arc<TraySharedState>) -> Self {
        Self { shared }
    }

    /// Attaches the signal emitter for KDE watcher signals.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        *self.shared.watcher_emitter.lock().await = Some(emitter);
    }
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl StatusNotifierWatcherServer {
    /// Signal emitted when a new item is registered.
    #[zbus(signal)]
    pub async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted when an item is unregistered.
    #[zbus(signal)]
    pub async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted when a new host registers.
    #[zbus(signal)]
    pub async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Signal emitted when the last host unregisters.
    #[zbus(signal)]
    pub async fn status_notifier_host_unregistered(emitter: &SignalEmitter<'_>)
        -> zbus::Result<()>;

    /// Array of currently registered item identifier paths.
    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.shared.registry.read().await.registered_item_ids()
    }

    /// Whether at least one visualization host is currently active.
    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        self.shared.registry.read().await.is_host_registered()
    }

    /// Implemented protocol version.
    #[zbus(property)]
    async fn protocol_version(&self) -> i32 {
        0
    }

    /// Registers a new StatusNotifierItem.
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        handle_register_item(Arc::clone(&self.shared), service, &header).await
    }

    /// Registers a visualization host.
    async fn register_status_notifier_host(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        handle_register_host(Arc::clone(&self.shared), service, &header).await
    }
}

/// Freedesktop `org.freedesktop.StatusNotifierWatcher` alias service endpoint.
#[derive(Clone)]
pub struct FdoStatusNotifierWatcherServer {
    shared: Arc<TraySharedState>,
}

impl FdoStatusNotifierWatcherServer {
    /// Creates a new FDO watcher server.
    #[must_use]
    pub fn new(shared: Arc<TraySharedState>) -> Self {
        Self { shared }
    }

    /// Attaches the signal emitter for FDO watcher signals.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        *self.shared.fdo_watcher_emitter.lock().await = Some(emitter);
    }
}

#[interface(name = "org.freedesktop.StatusNotifierWatcher")]
impl FdoStatusNotifierWatcherServer {
    /// Signal emitted when a new item is registered.
    #[zbus(signal)]
    pub async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted when an item is unregistered.
    #[zbus(signal)]
    pub async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted when a new host registers.
    #[zbus(signal)]
    pub async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Signal emitted when the last host unregisters.
    #[zbus(signal)]
    pub async fn status_notifier_host_unregistered(emitter: &SignalEmitter<'_>)
        -> zbus::Result<()>;

    /// Array of currently registered item identifier paths.
    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.shared.registry.read().await.registered_item_ids()
    }

    /// Whether at least one visualization host is currently active.
    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        self.shared.registry.read().await.is_host_registered()
    }

    /// Implemented protocol version.
    #[zbus(property)]
    async fn protocol_version(&self) -> i32 {
        0
    }

    /// Registers a new StatusNotifierItem.
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        handle_register_item(Arc::clone(&self.shared), service, &header).await
    }

    /// Registers a visualization host.
    async fn register_status_notifier_host(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        handle_register_host(Arc::clone(&self.shared), service, &header).await
    }
}

/// Internal `org.kynexos.Daemon.Tray1` service endpoint for `kynex-shell`.
#[derive(Clone)]
pub struct TrayServer {
    shared: Arc<TraySharedState>,
}

impl TrayServer {
    /// Creates a new Tray1 server.
    #[must_use]
    pub fn new(shared: Arc<TraySharedState>) -> Self {
        Self { shared }
    }

    /// Attaches the signal emitter for Tray1 signals.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        *self.shared.tray1_emitter.lock().await = Some(emitter);
    }
}

#[interface(name = "org.kynexos.Daemon.Tray1")]
impl TrayServer {
    /// Signal emitted when a new tray item is added.
    #[zbus(signal)]
    pub async fn item_added(emitter: &SignalEmitter<'_>, item: &TrayItemInfo) -> zbus::Result<()>;

    /// Signal emitted when an existing tray item changes.
    #[zbus(signal)]
    pub async fn item_changed(emitter: &SignalEmitter<'_>, item: &TrayItemInfo)
        -> zbus::Result<()>;

    /// Signal emitted when a tray item is unregistered or removed.
    #[zbus(signal)]
    pub async fn item_removed(emitter: &SignalEmitter<'_>, id: &str) -> zbus::Result<()>;

    /// Signal emitted when an item's menu layout updates.
    #[zbus(signal)]
    pub async fn menu_changed(
        emitter: &SignalEmitter<'_>,
        item_id: &str,
        revision: u32,
    ) -> zbus::Result<()>;

    /// Returns list of all active tray items.
    async fn get_items(&self) -> Vec<TrayItemInfo> {
        let registry = self.shared.registry.read().await;
        registry.iter().map(item_state_to_info).collect()
    }

    /// Invokes primary activation on target item.
    async fn activate(&self, id: &str, x: i32, y: i32) -> zbus::fdo::Result<()> {
        let (bus_name, object_path) = parse_item_id(id)?;
        let conn = get_connection(&self.shared).await?;
        let proxy = StatusNotifierItemProxy::builder(&conn)
            .destination(bus_name.as_str())
            .path(object_path.as_str())
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to build proxy: {e}")))?;

        tokio::time::timeout(
            Duration::from_secs(PROPERTY_TIMEOUT_SECS),
            proxy.activate(x, y),
        )
        .await
        .map_err(|_| zbus::fdo::Error::Timeout("Activate call timed out".to_string()))?
        .map_err(|e| zbus::fdo::Error::Failed(format!("Activate error: {e}")))?;

        Ok(())
    }

    /// Invokes secondary activation on target item.
    async fn secondary_activate(&self, id: &str, x: i32, y: i32) -> zbus::fdo::Result<()> {
        let (bus_name, object_path) = parse_item_id(id)?;
        let conn = get_connection(&self.shared).await?;
        let proxy = StatusNotifierItemProxy::builder(&conn)
            .destination(bus_name.as_str())
            .path(object_path.as_str())
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to build proxy: {e}")))?;

        tokio::time::timeout(
            Duration::from_secs(PROPERTY_TIMEOUT_SECS),
            proxy.secondary_activate(x, y),
        )
        .await
        .map_err(|_| zbus::fdo::Error::Timeout("SecondaryActivate call timed out".to_string()))?
        .map_err(|e| zbus::fdo::Error::Failed(format!("SecondaryActivate error: {e}")))?;

        Ok(())
    }

    /// Invokes context menu on target item with window coordinates.
    async fn context_menu(&self, id: &str, x: i32, y: i32) -> zbus::fdo::Result<()> {
        let (bus_name, object_path) = parse_item_id(id)?;
        let conn = get_connection(&self.shared).await?;
        let proxy = StatusNotifierItemProxy::builder(&conn)
            .destination(bus_name.as_str())
            .path(object_path.as_str())
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to build proxy: {e}")))?;

        tokio::time::timeout(
            Duration::from_secs(PROPERTY_TIMEOUT_SECS),
            proxy.context_menu(x, y),
        )
        .await
        .map_err(|_| zbus::fdo::Error::Timeout("ContextMenu call timed out".to_string()))?
        .map_err(|e| zbus::fdo::Error::Failed(format!("ContextMenu error: {e}")))?;

        Ok(())
    }

    /// Invokes scroll action on target item.
    async fn scroll(&self, id: &str, delta: i32, orientation: &str) -> zbus::fdo::Result<()> {
        let (bus_name, object_path) = parse_item_id(id)?;
        let conn = get_connection(&self.shared).await?;
        let proxy = StatusNotifierItemProxy::builder(&conn)
            .destination(bus_name.as_str())
            .path(object_path.as_str())
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to build proxy: {e}")))?;

        tokio::time::timeout(
            Duration::from_secs(PROPERTY_TIMEOUT_SECS),
            proxy.scroll(delta, orientation),
        )
        .await
        .map_err(|_| zbus::fdo::Error::Timeout("Scroll call timed out".to_string()))?
        .map_err(|e| zbus::fdo::Error::Failed(format!("Scroll error: {e}")))?;

        Ok(())
    }

    /// Fetches the current sanitized menu layout for target tray item.
    async fn get_menu(&self, item_id: &str) -> zbus::fdo::Result<(u32, Vec<MenuItemDto>)> {
        let (bus_name, object_path) = parse_item_id(item_id)?;
        let key = ItemKey::new(bus_name.clone(), object_path.clone());
        let menu_path = {
            let reg = self.shared.registry.read().await;
            let item = reg.get(&key).ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs(format!("Unknown tray item '{item_id}'"))
            })?;
            item.menu_path.clone().ok_or_else(|| {
                zbus::fdo::Error::NotSupported(format!(
                    "Tray item '{item_id}' does not export a dbusmenu"
                ))
            })?
        };

        let conn = get_connection(&self.shared).await?;
        self.shared
            .menu_manager
            .get_menu(&conn, &bus_name, &menu_path, item_id)
            .await
    }

    /// Signals that target submenu is about to be displayed; returns whether layout updated.
    async fn menu_about_to_show(
        &self,
        item_id: &str,
        menu_item_id: i32,
    ) -> zbus::fdo::Result<bool> {
        let (bus_name, object_path) = parse_item_id(item_id)?;
        let key = ItemKey::new(bus_name.clone(), object_path.clone());
        let menu_path = {
            let reg = self.shared.registry.read().await;
            let item = reg.get(&key).ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs(format!("Unknown tray item '{item_id}'"))
            })?;
            item.menu_path.clone().ok_or_else(|| {
                zbus::fdo::Error::NotSupported(format!(
                    "Tray item '{item_id}' does not export a dbusmenu"
                ))
            })?
        };

        let conn = get_connection(&self.shared).await?;
        self.shared
            .menu_manager
            .menu_about_to_show(&conn, &bus_name, &menu_path, item_id, menu_item_id)
            .await
    }

    /// Dispatches a user interaction event to the item's dbusmenu service.
    async fn menu_event(
        &self,
        item_id: &str,
        menu_item_id: i32,
        event: &str,
        revision: u32,
    ) -> zbus::fdo::Result<()> {
        let (bus_name, object_path) = parse_item_id(item_id)?;
        let key = ItemKey::new(bus_name.clone(), object_path.clone());
        let menu_path = {
            let reg = self.shared.registry.read().await;
            let item = reg.get(&key).ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs(format!("Unknown tray item '{item_id}'"))
            })?;
            item.menu_path.clone().ok_or_else(|| {
                zbus::fdo::Error::NotSupported(format!(
                    "Tray item '{item_id}' does not export a dbusmenu"
                ))
            })?
        };

        let conn = get_connection(&self.shared).await?;
        self.shared
            .menu_manager
            .menu_event(
                &conn,
                &bus_name,
                &menu_path,
                item_id,
                menu_item_id,
                event,
                revision,
            )
            .await
    }
}

/// Helper converting pure Rust [`ItemState`] to D-Bus IPC [`TrayItemInfo`].
#[must_use]
pub fn item_state_to_info(state: &ItemState) -> TrayItemInfo {
    let (icon_pixmap, icon_pixmap_width, icon_pixmap_height) = match &state.icon_pixmap {
        Some(p) => (p.rgba_data.clone(), p.width, p.height),
        None => (Vec::new(), 0, 0),
    };
    let (attention_pixmap, attention_pixmap_width, attention_pixmap_height) =
        match &state.attention_pixmap {
            Some(p) => (p.rgba_data.clone(), p.width, p.height),
            None => (Vec::new(), 0, 0),
        };

    TrayItemInfo {
        id: state.key.as_id_string(),
        title: state.title.clone(),
        status: state.status.as_str().to_string(),
        icon_name: state.icon_name.clone().unwrap_or_default(),
        icon_theme_path: state.icon_theme_path.clone().unwrap_or_default(),
        icon_pixmap,
        icon_pixmap_width,
        icon_pixmap_height,
        tooltip_title: state.tooltip_title.clone(),
        tooltip_text: state.tooltip_text.clone(),
        is_menu: state.is_menu,
        window_id: state.window_id,
        has_attention: state.status == ItemStatus::NeedsAttention,
        attention_icon_name: state.attention_icon_name.clone().unwrap_or_default(),
        attention_pixmap,
        attention_pixmap_width,
        attention_pixmap_height,
        menu_path: state.menu_path.clone().unwrap_or_default(),
    }
}

/// Parses canonical item identifier string `"<bus_name><object_path>"` into `(bus_name, object_path)`.
fn parse_item_id(id: &str) -> zbus::fdo::Result<(String, String)> {
    if let Some(idx) = id.find('/') {
        let (bus, path) = id.split_at(idx);
        if bus.is_empty() || path.is_empty() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Malformed item ID".to_string(),
            ));
        }
        Ok((bus.to_string(), path.to_string()))
    } else {
        Err(zbus::fdo::Error::InvalidArgs(
            "Malformed item ID: missing path separator".to_string(),
        ))
    }
}

/// Retrieves the active D-Bus connection.
async fn get_connection(shared: &TraySharedState) -> zbus::fdo::Result<Connection> {
    shared
        .conn
        .read()
        .await
        .clone()
        .ok_or_else(|| zbus::fdo::Error::Failed("D-Bus connection not initialized".to_string()))
}

/// Resolves registration, verifies sender authenticity, checks capacity, fetches properties,
/// registers the item, and emits change signals.
async fn handle_register_item(
    shared: Arc<TraySharedState>,
    service_or_path: &str,
    header: &Header<'_>,
) -> zbus::fdo::Result<()> {
    let sender = header
        .sender()
        .ok_or_else(|| zbus::fdo::Error::Failed("Missing message sender".to_string()))?
        .as_str();

    let conn = get_connection(&shared).await?;
    let dbus_proxy = DBusProxy::new(&conn)
        .await
        .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to connect to dbus daemon: {e}")))?;

    let trimmed = service_or_path.trim();
    if trimmed.is_empty() {
        return Err(zbus::fdo::Error::InvalidArgs(
            "Service or path argument cannot be empty".to_string(),
        ));
    }

    let (target_service, object_path) = if trimmed.starts_with('/') {
        (sender.to_string(), trimmed.to_string())
    } else if let Some(idx) = trimmed.find('/') {
        let (svc, path) = trimmed.split_at(idx);
        (svc.to_string(), path.to_string())
    } else {
        (trimmed.to_string(), "/StatusNotifierItem".to_string())
    };

    if !object_path.starts_with('/') || object_path.contains(' ') || object_path.contains('\0') {
        return Err(zbus::fdo::Error::InvalidArgs(format!(
            "Invalid object path: {object_path}"
        )));
    }

    // Authenticate sender
    let well_known_name: Option<String> = if target_service.starts_with(':') {
        if target_service != sender {
            return Err(zbus::fdo::Error::AccessDenied(
                "Sender does not own the registered service name".to_string(),
            ));
        }
        None
    } else {
        let bus_name_arg = target_service
            .as_str()
            .try_into()
            .map_err(|_| zbus::fdo::Error::InvalidArgs("Invalid well-known name".to_string()))?;
        let owner = match dbus_proxy.get_name_owner(bus_name_arg).await {
            Ok(owner) => owner.as_str().to_string(),
            Err(_) => {
                return Err(zbus::fdo::Error::AccessDenied(
                    "Service name is not owned".to_string(),
                ));
            }
        };
        if owner != sender {
            return Err(zbus::fdo::Error::AccessDenied(
                "Sender does not own the registered service name".to_string(),
            ));
        }
        Some(target_service)
    };

    let key = ItemKey::new(sender, object_path);

    // Check capacity limits
    {
        let reg = shared.registry.read().await;
        if let Err(e) = reg.check_capacity(&key) {
            return match e {
                TrayError::SenderLimitExceeded(max) => Err(zbus::fdo::Error::LimitsExceeded(
                    format!("Sender limit exceeded: max {max} items per client"),
                )),
                TrayError::TotalLimitExceeded(max) => Err(zbus::fdo::Error::LimitsExceeded(
                    format!("Total limit exceeded: max {max} items total"),
                )),
                _ => Err(zbus::fdo::Error::Failed(e.to_string())),
            };
        }
    }

    // Asynchronously fetch item properties with a 2-second timeout and bounded concurrency
    let item_state = {
        let _permit = shared.fetch_semaphore.acquire().await.map_err(|e| {
            zbus::fdo::Error::Failed(format!("Failed to acquire fetch permit: {e}"))
        })?;
        fetch_item_properties(&conn, &key).await?
    };

    // Upsert into registry
    {
        let mut reg = shared.registry.write().await;
        if let Err(e) = reg.upsert(item_state.clone()) {
            return Err(zbus::fdo::Error::LimitsExceeded(e.to_string()));
        }
    }

    if let Some(wk_name) = well_known_name {
        let mut wk_map = shared.well_known_owners.lock().await;
        let list = wk_map.entry(wk_name).or_default();
        if !list.contains(&key) {
            list.push(key.clone());
        }
    }

    let item_id_str = key.as_id_string();
    let info = item_state_to_info(&item_state);

    // Emit watcher signals
    if let Some(emitter) = shared.watcher_emitter.lock().await.as_ref() {
        let _ = StatusNotifierWatcherServer::status_notifier_item_registered(emitter, &item_id_str)
            .await;
    }
    if let Some(emitter) = shared.fdo_watcher_emitter.lock().await.as_ref() {
        let _ =
            FdoStatusNotifierWatcherServer::status_notifier_item_registered(emitter, &item_id_str)
                .await;
    }

    // Emit internal Tray1 signal
    if let Some(emitter) = shared.tray1_emitter.lock().await.as_ref() {
        let _ = TrayServer::item_added(emitter, &info).await;
    }

    info!(
        "Registered tray item '{}' for sender '{}'",
        item_id_str, sender
    );

    // Cancel any previous signal listener for this key
    if let Some(old_tx) = shared.item_cancellers.lock().await.remove(&key) {
        let _ = old_tx.send(());
    }

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    shared
        .item_cancellers
        .lock()
        .await
        .insert(key.clone(), cancel_tx);

    // Spawn item signal listener
    tokio::spawn(run_item_signal_listener(
        Arc::clone(&shared),
        conn.clone(),
        key.clone(),
        cancel_rx,
    ));

    // Register dbusmenu signal listener if item exports a menu path
    if let Some(ref menu_path) = item_state.menu_path {
        shared
            .menu_manager
            .register_signal_listener(&conn, &key, menu_path, Arc::clone(&shared.tray1_emitter))
            .await;
    }

    Ok(())
}

/// Registers a visualization panel host.
async fn handle_register_host(
    shared: Arc<TraySharedState>,
    _service: &str,
    header: &Header<'_>,
) -> zbus::fdo::Result<()> {
    let sender = header
        .sender()
        .ok_or_else(|| zbus::fdo::Error::Failed("Missing message sender".to_string()))?
        .as_str();

    let is_first = shared.registry.write().await.register_host(sender);
    if is_first {
        info!("First visualization host registered by '{sender}'");
        if let Some(emitter) = shared.watcher_emitter.lock().await.as_ref() {
            let _ = StatusNotifierWatcherServer::status_notifier_host_registered(emitter).await;
        }
        if let Some(emitter) = shared.fdo_watcher_emitter.lock().await.as_ref() {
            let _ = FdoStatusNotifierWatcherServer::status_notifier_host_registered(emitter).await;
        }
    }

    Ok(())
}

/// Asynchronously fetches and sanitizes properties from target StatusNotifierItem with 2s timeout
/// using a single `org.freedesktop.DBus.Properties.GetAll` call.
async fn fetch_item_properties(conn: &Connection, key: &ItemKey) -> zbus::fdo::Result<ItemState> {
    let props_proxy = PropertiesProxy::builder(conn)
        .destination(key.bus_name.as_str())
        .path(key.object_path.as_str())
        .build()
        .await
        .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to build properties proxy: {e}")))?;

    let fetch_fut = async {
        let kde_iface = "org.kde.StatusNotifierItem"
            .try_into()
            .map_err(|e| zbus::fdo::Error::Failed(format!("Invalid interface name: {e}")))?;
        let props = if let Ok(p) = props_proxy.get_all(kde_iface).await {
            p
        } else {
            let fdo_iface = "org.freedesktop.StatusNotifierItem"
                .try_into()
                .map_err(|e| zbus::fdo::Error::Failed(format!("Invalid interface name: {e}")))?;
            props_proxy.get_all(fdo_iface).await.map_err(|e| {
                zbus::fdo::Error::Failed(format!("Failed to fetch properties via GetAll: {e}"))
            })?
        };

        let raw_id: String = props
            .get("Id")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default();
        let raw_title: String = props
            .get("Title")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default();
        let raw_status: String = props
            .get("Status")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| "Active".to_string());
        let window_id: i32 = props
            .get("WindowId")
            .and_then(|v| {
                v.clone().try_into().ok().or_else(|| {
                    let u: Option<u32> = v.clone().try_into().ok();
                    u.map(|val| val.cast_signed())
                })
            })
            .unwrap_or(0);
        let raw_icon_theme_path: Option<String> = props
            .get("IconThemePath")
            .and_then(|v| v.clone().try_into().ok());
        let raw_icon_name: String = props
            .get("IconName")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default();
        let raw_pixmaps: Vec<RawPixmapTuple> = props
            .get("IconPixmap")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default();
        let raw_attention_icon_name: String = props
            .get("AttentionIconName")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default();
        let raw_attention_pixmaps: Vec<RawPixmapTuple> = props
            .get("AttentionIconPixmap")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default();
        let is_menu: bool = props
            .get("ItemIsMenu")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or(false);

        let raw_menu_path: Option<String> = props.get("Menu").and_then(|v| {
            let p = match &**v {
                zbus::zvariant::Variant::ObjectPath(op) => op.as_str().trim(),
                zbus::zvariant::Variant::Str(s) => s.as_str().trim(),
                _ => return None,
            };
            if p.is_empty() || p == "/" || p == "/NO_DBUSMENU" {
                None
            } else {
                Some(p.to_string())
            }
        });

        let (raw_tt_title, raw_tt_text) = match props.get("ToolTip").and_then(|v| {
            let tt: Result<(String, Vec<RawPixmapTuple>, String, String), _> = v.clone().try_into();
            tt.ok()
        }) {
            Some((_, _, t_title, t_text)) => (t_title, t_text),
            None => (String::new(), String::new()),
        };

        Ok::<_, zbus::fdo::Error>((
            raw_id,
            raw_title,
            raw_status,
            window_id,
            raw_icon_theme_path,
            raw_icon_name,
            raw_pixmaps,
            raw_attention_icon_name,
            raw_attention_pixmaps,
            is_menu,
            raw_menu_path,
            raw_tt_title,
            raw_tt_text,
        ))
    };

    let (
        raw_id,
        raw_title,
        raw_status,
        window_id,
        raw_icon_theme_path,
        raw_icon_name,
        raw_pixmaps,
        raw_attention_icon_name,
        raw_attention_pixmaps,
        is_menu,
        raw_menu_path,
        raw_tt_title,
        raw_tt_text,
    ) = tokio::time::timeout(Duration::from_secs(PROPERTY_TIMEOUT_SECS), fetch_fut)
        .await
        .map_err(|_| {
            zbus::fdo::Error::Timeout("Timed out fetching item properties after 2s".to_string())
        })??;

    let item_id = sanitize_id(&raw_id);
    let title = sanitize_title(&raw_title);
    let status = ItemStatus::parse_or_default(&raw_status);
    let icon_name = sanitize_icon_name(&raw_icon_name);
    let icon_theme_path = raw_icon_theme_path.and_then(|p| {
        let trimmed = p.trim();
        if trimmed.is_empty() || trimmed.len() > 256 || trimmed.contains("..") {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    let raw_tuples: Vec<RawPixmapTuple> = raw_pixmaps;
    let icon_pixmap = match select_best_pixmap(&raw_tuples) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "Rejected hostile icon pixmap from '{}': {e}",
                key.as_id_string()
            );
            None
        }
    };

    let attention_icon_name = sanitize_icon_name(&raw_attention_icon_name);
    let raw_att_tuples: Vec<RawPixmapTuple> = raw_attention_pixmaps;
    let attention_pixmap = match select_best_pixmap(&raw_att_tuples) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "Rejected hostile attention pixmap from '{}': {e}",
                key.as_id_string()
            );
            None
        }
    };

    let tooltip_title = sanitize_tooltip_title(&raw_tt_title);
    let tooltip_text = sanitize_tooltip_text(&raw_tt_text);

    Ok(ItemState {
        key: key.clone(),
        item_id,
        title,
        status,
        icon_name,
        icon_theme_path,
        icon_pixmap,
        tooltip_title,
        tooltip_text,
        is_menu,
        window_id,
        attention_icon_name,
        attention_pixmap,
        menu_path: raw_menu_path,
    })
}

/// Listens for item property change signals and triggers rate-limited refresh.
///
/// Signal ingestion is decoupled from property fetching via a channel of size 1 (coalescing).
/// The stream reader loop never blocks on D-Bus fetches, preventing connection congestion.
/// Background fetch tasks are capped by `fetch_semaphore` and time-bounded by the 2s timeout.
async fn run_item_signal_listener(
    shared: Arc<TraySharedState>,
    conn: Connection,
    key: ItemKey,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let bus_name = key.bus_name.clone();
    let object_path = key.object_path.clone();

    // Match signals emitted by this specific item
    let Ok(rule) = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(bus_name.as_str())
        .path(object_path.as_str())
        .build()
    else {
        return;
    };

    let mut stream = match MessageStream::for_match_rule(rule, &conn, None).await {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to create signal stream for tray item: {e}");
            return;
        }
    };

    let item_id_str = key.as_id_string();

    // Channel of size 1 for coalescing incoming refresh signals
    let (dirty_tx, mut dirty_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Spawn the background coalesced property fetch worker
    let worker_shared = Arc::clone(&shared);
    let worker_conn = conn.clone();
    let worker_key = key.clone();
    let worker_item_id = item_id_str.clone();
    let worker_dirty_tx = dirty_tx.clone();

    let fetch_worker = tokio::spawn(async move {
        while dirty_rx.recv().await.is_some() {
            // Check if item is still tracked
            if worker_shared
                .registry
                .read()
                .await
                .get(&worker_key)
                .is_none()
            {
                break;
            }

            // Evaluate token-bucket rate limit (10/s)
            let now = Instant::now();
            let allowed = worker_shared
                .rate_limiter
                .lock()
                .await
                .check_and_consume(&worker_item_id, now)
                .is_ok();

            if !allowed {
                debug!("Rate limit reached for '{worker_item_id}', debouncing trailing fetch");
                tokio::time::sleep(Duration::from_millis(100)).await;
                // Re-mark dirty to guarantee final state is fetched
                let _ = worker_dirty_tx.try_send(());
                continue;
            }

            // Acquire bounded concurrency permit
            let Ok(permit) = worker_shared.fetch_semaphore.acquire().await else {
                break;
            };

            // Fetch properties with 2s timeout
            if let Ok(updated_state) = fetch_item_properties(&worker_conn, &worker_key).await {
                drop(permit);

                let mut reg = worker_shared.registry.write().await;
                if reg.get(&worker_key).is_some() {
                    let info = item_state_to_info(&updated_state);
                    let _ = reg.upsert(updated_state);
                    drop(reg);

                    if let Some(emitter) = worker_shared.tray1_emitter.lock().await.as_ref() {
                        let _ = TrayServer::item_changed(emitter, &info).await;
                    }
                }
            }
        }
    });

    // Stream reader loop: purely ingests signals and triggers coalesced dirty notifications
    loop {
        let msg = tokio::select! {
            _ = &mut cancel_rx => {
                debug!("Cancelled signal listener for '{item_id_str}'");
                break;
            }
            msg = stream.next() => {
                match msg {
                    Some(m) => m,
                    None => break,
                }
            }
        };

        let Ok(msg) = msg else { continue };
        let header = msg.header();
        let member = header.member().map(|m| m.as_str()).unwrap_or_default();

        let is_refresh_signal = matches!(
            member,
            "NewTitle"
                | "NewIcon"
                | "NewAttentionIcon"
                | "NewOverlayIcon"
                | "NewToolTip"
                | "NewStatus"
                | "PropertiesChanged"
        );

        if !is_refresh_signal {
            continue;
        }

        // Notify coalescing channel (non-blocking)
        let _ = dirty_tx.try_send(());
    }

    // Terminate worker
    fetch_worker.abort();
    shared.item_cancellers.lock().await.remove(&key);
}

/// Runs the D-Bus NameOwnerChanged listener to automatically remove tray items
/// and unregister visualization hosts when clients disconnect or release well-known names.
pub async fn run_tray_dbus_name_watcher(shared: Arc<TraySharedState>, conn: Connection) {
    let Ok(rule) = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")
        .path("/org/freedesktop/DBus")
        .interface("org.freedesktop.DBus")
        .member("NameOwnerChanged")
        .build()
    else {
        return;
    };

    let mut stream = match MessageStream::for_match_rule(rule, &conn, None).await {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to create NameOwnerChanged stream for tray: {e}");
            return;
        }
    };

    while let Some(msg) = stream.next().await {
        let Ok(msg) = msg else { continue };
        let Ok((name, old_owner, new_owner)) = msg.body().deserialize::<(String, String, String)>()
        else {
            continue;
        };

        // Case 1: A client's unique bus name disconnected (new_owner is empty)
        if name.starts_with(':') && new_owner.is_empty() {
            let removed_items = {
                let mut reg = shared.registry.write().await;
                reg.remove_sender(&name)
            };

            // Clean up any well-known name mappings for these removed items
            {
                let mut wk_map = shared.well_known_owners.lock().await;
                for item in &removed_items {
                    for items in wk_map.values_mut() {
                        items.retain(|k| k != &item.key);
                    }
                }
                wk_map.retain(|_, v| !v.is_empty());
            }

            for item in removed_items {
                let id = item.key.as_id_string();
                info!("Tray item owner vanished: removing '{id}'");
                shared.rate_limiter.lock().await.remove(&id);
                if let Some(tx) = shared.item_cancellers.lock().await.remove(&item.key) {
                    let _ = tx.send(());
                }

                if let Some(emitter) = shared.watcher_emitter.lock().await.as_ref() {
                    let _ = StatusNotifierWatcherServer::status_notifier_item_unregistered(
                        emitter, &id,
                    )
                    .await;
                }
                if let Some(emitter) = shared.fdo_watcher_emitter.lock().await.as_ref() {
                    let _ = FdoStatusNotifierWatcherServer::status_notifier_item_unregistered(
                        emitter, &id,
                    )
                    .await;
                }
                if let Some(emitter) = shared.tray1_emitter.lock().await.as_ref() {
                    let _ = TrayServer::item_removed(emitter, &id).await;
                }
                shared.menu_manager.on_item_removed(&id).await;
            }

            // Check if name was a host
            let last_host_left = {
                let mut reg = shared.registry.write().await;
                reg.unregister_host(&name)
            };

            if last_host_left {
                info!("Last tray host vanished ('{name}')");
                if let Some(emitter) = shared.watcher_emitter.lock().await.as_ref() {
                    let _ = StatusNotifierWatcherServer::status_notifier_host_unregistered(emitter)
                        .await;
                }
                if let Some(emitter) = shared.fdo_watcher_emitter.lock().await.as_ref() {
                    let _ =
                        FdoStatusNotifierWatcherServer::status_notifier_host_unregistered(emitter)
                            .await;
                }
            }
        } else if !name.starts_with(':') && new_owner != old_owner {
            // Case 2: A well-known service name changed owners or was released
            let keys_to_remove = {
                let mut wk_map = shared.well_known_owners.lock().await;
                wk_map.remove(&name).unwrap_or_default()
            };

            for key in keys_to_remove {
                let removed = {
                    let mut reg = shared.registry.write().await;
                    reg.unregister(&key)
                };

                if let Some(item) = removed {
                    let id = item.key.as_id_string();
                    info!("Well-known name '{name}' owner changed: removing '{id}'");
                    shared.rate_limiter.lock().await.remove(&id);
                    if let Some(tx) = shared.item_cancellers.lock().await.remove(&item.key) {
                        let _ = tx.send(());
                    }

                    if let Some(emitter) = shared.watcher_emitter.lock().await.as_ref() {
                        let _ = StatusNotifierWatcherServer::status_notifier_item_unregistered(
                            emitter, &id,
                        )
                        .await;
                    }
                    if let Some(emitter) = shared.fdo_watcher_emitter.lock().await.as_ref() {
                        let _ = FdoStatusNotifierWatcherServer::status_notifier_item_unregistered(
                            emitter, &id,
                        )
                        .await;
                    }
                    if let Some(emitter) = shared.tray1_emitter.lock().await.as_ref() {
                        let _ = TrayServer::item_removed(emitter, &id).await;
                    }
                    shared.menu_manager.on_item_removed(&id).await;
                }
            }
        }
    }
}

/// Attempts to acquire `org.kde.StatusNotifierWatcher` and `org.freedesktop.StatusNotifierWatcher`
/// using `DO_NOT_QUEUE` (no replacement).
///
/// Returns `(kde_acquired, fdo_acquired)`.
pub async fn try_acquire_watcher_names(conn: &Connection) -> (bool, bool) {
    let kde = match conn
        .request_name_with_flags(
            STATUS_NOTIFIER_WATCHER_DESTINATION_KDE,
            zbus::fdo::RequestNameFlags::DO_NOT_QUEUE,
        )
        .await
    {
        Ok(
            zbus::fdo::RequestNameReply::PrimaryOwner | zbus::fdo::RequestNameReply::AlreadyOwner,
        ) => {
            info!(
                "Successfully claimed well-known name '{STATUS_NOTIFIER_WATCHER_DESTINATION_KDE}'"
            );
            true
        }
        Ok(_) | Err(zbus::Error::NameTaken) => {
            warn!(
                "Name '{STATUS_NOTIFIER_WATCHER_DESTINATION_KDE}' is already owned by another service. Disabling KDE watcher endpoint on this bus."
            );
            false
        }
        Err(e) => {
            warn!("Failed to request '{STATUS_NOTIFIER_WATCHER_DESTINATION_KDE}': {e}");
            false
        }
    };

    let fdo = match conn
        .request_name_with_flags(
            STATUS_NOTIFIER_WATCHER_DESTINATION_FDO,
            zbus::fdo::RequestNameFlags::DO_NOT_QUEUE,
        )
        .await
    {
        Ok(
            zbus::fdo::RequestNameReply::PrimaryOwner | zbus::fdo::RequestNameReply::AlreadyOwner,
        ) => {
            info!(
                "Successfully claimed well-known name '{STATUS_NOTIFIER_WATCHER_DESTINATION_FDO}'"
            );
            true
        }
        Ok(_) | Err(zbus::Error::NameTaken) => {
            warn!(
                "Name '{STATUS_NOTIFIER_WATCHER_DESTINATION_FDO}' is already owned by another service. Disabling Freedesktop watcher endpoint on this bus."
            );
            false
        }
        Err(e) => {
            warn!("Failed to request '{STATUS_NOTIFIER_WATCHER_DESTINATION_FDO}': {e}");
            false
        }
    };

    (kde, fdo)
}
