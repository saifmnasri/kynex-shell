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
    StatusNotifierItemProxy, TrayItemInfo, STATUS_NOTIFIER_WATCHER_DESTINATION_FDO,
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
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection, MatchRule, MessageStream};

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
    pub tray1_emitter: Mutex<Option<SignalEmitter<'static>>>,
    /// Whether `org.kde.StatusNotifierWatcher` was successfully acquired on the bus.
    pub kde_name_acquired: AtomicBool,
    /// Whether `org.freedesktop.StatusNotifierWatcher` was successfully acquired on the bus.
    pub fdo_name_acquired: AtomicBool,
    /// Cancellation senders for active item signal listener tasks.
    pub item_cancellers: Mutex<HashMap<ItemKey, tokio::sync::oneshot::Sender<()>>>,
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
            tray1_emitter: Mutex::new(None),
            kde_name_acquired: AtomicBool::new(false),
            fdo_name_acquired: AtomicBool::new(false),
            item_cancellers: Mutex::new(HashMap::new()),
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

    // Authenticate sender
    let key = TrayRegistry::resolve_registration(sender, service_or_path, |name| {
        // Synchronously or blockingly query owner; in async we can query via dbus proxy
        // Since resolve_registration accepts a closure, we resolve it first
        let name_owned = name.to_string();
        // Fallback for closure: caller handles lookup below
        Some(name_owned)
    });

    // Check if well-known name lookup needed
    let key = match key {
        Ok(k) => {
            // If the service was a well-known name, verify actual bus owner
            if !service_or_path.trim().starts_with('/') && !service_or_path.trim().starts_with(':')
            {
                let svc_name = if let Some(idx) = service_or_path.find('/') {
                    &service_or_path[..idx]
                } else {
                    service_or_path.trim()
                };
                match dbus_proxy
                    .get_name_owner(svc_name.try_into().map_err(|_| {
                        zbus::fdo::Error::InvalidArgs("Invalid well-known name".to_string())
                    })?)
                    .await
                {
                    Ok(owner) if owner.as_str() == sender => k,
                    _ => {
                        return Err(zbus::fdo::Error::AccessDenied(
                            "Sender does not own the registered service name".to_string(),
                        ));
                    }
                }
            } else {
                k
            }
        }
        Err(TrayError::SenderMismatch) => {
            return Err(zbus::fdo::Error::AccessDenied(
                "Sender does not own the registered service name".to_string(),
            ));
        }
        Err(TrayError::InvalidObjectPath(p)) => {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Invalid object path: {p}"
            )));
        }
        Err(e) => {
            return Err(zbus::fdo::Error::Failed(e.to_string()));
        }
    };

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

    // Asynchronously fetch item properties with a 2-second timeout
    let item_state = fetch_item_properties(&conn, &key).await?;

    // Upsert into registry
    {
        let mut reg = shared.registry.write().await;
        if let Err(e) = reg.upsert(item_state.clone()) {
            return Err(zbus::fdo::Error::LimitsExceeded(e.to_string()));
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
        conn,
        key,
        cancel_rx,
    ));

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

/// Asynchronously fetches and sanitizes properties from target StatusNotifierItem with 2s timeout.
async fn fetch_item_properties(conn: &Connection, key: &ItemKey) -> zbus::fdo::Result<ItemState> {
    let proxy = StatusNotifierItemProxy::builder(conn)
        .destination(key.bus_name.as_str())
        .path(key.object_path.as_str())
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to build item proxy: {e}")))?;

    let fetch_fut = async {
        let raw_id = proxy.id().await.unwrap_or_default();
        let raw_title = proxy.title().await.unwrap_or_default();
        let raw_status = proxy
            .status()
            .await
            .unwrap_or_else(|_| "Active".to_string());
        let window_id = proxy.window_id().await.unwrap_or(0);
        let raw_icon_theme_path = proxy.icon_theme_path().await.ok();
        let raw_icon_name = proxy.icon_name().await.unwrap_or_default();
        let raw_pixmaps = proxy.icon_pixmap().await.unwrap_or_default();
        let raw_attention_icon_name = proxy.attention_icon_name().await.unwrap_or_default();
        let raw_attention_pixmaps = proxy.attention_icon_pixmap().await.unwrap_or_default();
        let is_menu = proxy.item_is_menu().await.unwrap_or(false);

        let (tt_title, tt_text) = match proxy.tool_tip().await {
            Ok((_, _, t_title, t_text)) => (t_title, t_text),
            Err(_) => (String::new(), String::new()),
        };

        (
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
            tt_title,
            tt_text,
        )
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
        raw_tt_title,
        raw_tt_text,
    ) = tokio::time::timeout(Duration::from_secs(PROPERTY_TIMEOUT_SECS), fetch_fut)
        .await
        .map_err(|_| {
            zbus::fdo::Error::Timeout("Timed out fetching item properties after 2s".to_string())
        })?;

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
    })
}

/// Listens for item property change signals and triggers rate-limited refresh.
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

        // Check if item is still tracked
        let tracked = shared.registry.read().await.get(&key).is_some();
        if !tracked {
            break;
        }

        // Evaluate token-bucket rate limit (10/s)
        let now = Instant::now();
        let allowed = shared
            .rate_limiter
            .lock()
            .await
            .check_and_consume(&item_id_str, now)
            .is_ok();

        if !allowed {
            debug!(
                "Dropped rapid refresh signal '{member}' for '{item_id_str}' (rate limit exceeded)"
            );
            continue;
        }

        // Re-fetch properties with 2s timeout
        if let Ok(updated_state) = fetch_item_properties(&conn, &key).await {
            let mut reg = shared.registry.write().await;
            if reg.get(&key).is_some() {
                let info = item_state_to_info(&updated_state);
                let _ = reg.upsert(updated_state);

                drop(reg);

                if let Some(emitter) = shared.tray1_emitter.lock().await.as_ref() {
                    let _ = TrayServer::item_changed(emitter, &info).await;
                }
            }
        }
    }

    shared.item_cancellers.lock().await.remove(&key);
}

/// Runs the D-Bus NameOwnerChanged listener to automatically remove tray items
/// and unregister visualization hosts when clients disconnect.
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
        let Ok((name, _old_owner, new_owner)) =
            msg.body().deserialize::<(String, String, String)>()
        else {
            continue;
        };

        // Disconnect occurs when new_owner is empty
        if !new_owner.is_empty() {
            continue;
        }

        // 1. Check if name was an item sender
        let removed_items = {
            let mut reg = shared.registry.write().await;
            reg.remove_sender(&name)
        };

        for item in removed_items {
            let id = item.key.as_id_string();
            info!("Tray item owner vanished: removing '{id}'");
            shared.rate_limiter.lock().await.remove(&id);
            if let Some(tx) = shared.item_cancellers.lock().await.remove(&item.key) {
                let _ = tx.send(());
            }

            if let Some(emitter) = shared.watcher_emitter.lock().await.as_ref() {
                let _ =
                    StatusNotifierWatcherServer::status_notifier_item_unregistered(emitter, &id)
                        .await;
            }
            if let Some(emitter) = shared.fdo_watcher_emitter.lock().await.as_ref() {
                let _ =
                    FdoStatusNotifierWatcherServer::status_notifier_item_unregistered(emitter, &id)
                        .await;
            }
            if let Some(emitter) = shared.tray1_emitter.lock().await.as_ref() {
                let _ = TrayServer::item_removed(emitter, &id).await;
            }
        }

        // 2. Check if name was a host
        let last_host_left = {
            let mut reg = shared.registry.write().await;
            reg.unregister_host(&name)
        };

        if last_host_left {
            info!("Last tray host vanished ('{name}')");
            if let Some(emitter) = shared.watcher_emitter.lock().await.as_ref() {
                let _ =
                    StatusNotifierWatcherServer::status_notifier_host_unregistered(emitter).await;
            }
            if let Some(emitter) = shared.fdo_watcher_emitter.lock().await.as_ref() {
                let _ = FdoStatusNotifierWatcherServer::status_notifier_host_unregistered(emitter)
                    .await;
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
