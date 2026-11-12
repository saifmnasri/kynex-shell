//! Host-side client and manager for `com.canonical.dbusmenu`.
//!
//! Enforces:
//! - 2-second timeout on all remote calls.
//! - Per-item fetch coalescing and a cap on concurrent fetches (max 8).
//! - Per-item rate limiting on menu refreshes (10 refreshes/second).
//! - Revision-based layout caching with on-demand lazy loading.
//! - Automatic cache invalidation on item unregistration or bus owner disconnect.
//! - Event validation against the active layout before forwarding to the remote service.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kynex_dbusmenu::{
    parse_menu_layout, validate_menu_event, RawMenuNode, RawValue, ValidatedMenuItem,
    ValidatedMenuLayout, MAX_CONCURRENT_FETCHES, MAX_MENU_DEPTH, MENU_TIMEOUT_SECS,
};
use kynex_ipc::tray::{DBusMenuProxy, MenuItemDto};
use kynex_tray::registry::ItemKey;
use kynex_tray::RefreshRateLimiter;
use tokio::sync::{broadcast, Mutex, RwLock, Semaphore};
use tracing::{debug, warn};
use zbus::object_server::SignalEmitter;
use zbus::{Connection, Variant};

/// Cached menu entry holding the validated layout and ready-to-serve DTOs.
#[derive(Debug, Clone)]
pub struct MenuCacheEntry {
    /// Monotonic revision number of the layout.
    pub revision: u32,
    /// Validated layout structure.
    pub layout: ValidatedMenuLayout,
    /// Pre-computed list of item DTOs.
    pub dtos: Vec<MenuItemDto>,
}

/// Converts a D-Bus variant to an untrusted pure-Rust [`RawValue`].
#[must_use]
pub fn variant_to_raw_value(val: &Variant<'_>) -> RawValue {
    match val {
        Variant::Str(s) => RawValue::String(s.as_str().to_string()),
        Variant::Bool(b) => RawValue::Bool(*b),
        Variant::I16(i) => RawValue::Int(i64::from(*i)),
        Variant::I32(i) => RawValue::Int(i64::from(*i)),
        Variant::I64(i) => RawValue::Int(*i),
        Variant::U8(u) => RawValue::Int(i64::from(*u)),
        Variant::U16(u) => RawValue::Int(i64::from(*u)),
        Variant::U32(u) => RawValue::Int(i64::from(*u)),
        Variant::U64(u) => match i64::try_from(*u) {
            Ok(v) => RawValue::Int(v),
            Err(_) => RawValue::Int(i64::MAX),
        },
        Variant::Array(arr) => {
            let mut items = Vec::with_capacity(arr.len());
            for item in arr.iter() {
                items.push(variant_to_raw_value(item));
            }
            RawValue::Array(items)
        }
        Variant::Variant(inner) => variant_to_raw_value(inner.as_ref()),
        _ => RawValue::Other,
    }
}

/// Converts a D-Bus [`zbus::OwnedVariant`] to an untrusted pure-Rust [`RawValue`].
#[must_use]
pub fn owned_variant_to_raw_value(val: &zbus::OwnedVariant) -> RawValue {
    variant_to_raw_value(val)
}

/// Recursively converts raw D-Bus layout data into a [`RawMenuNode`].
#[must_use]
fn unwrap_variant<'a>(mut v: &'a Variant<'a>) -> &'a Variant<'a> {
    while let Variant::Variant(inner) = v {
        v = inner.as_ref();
    }
    v
}

/// Recursively converts raw D-Bus layout data into a [`RawMenuNode`], bounded to prevent stack overflow.
#[must_use]
#[allow(clippy::implicit_hasher)]
pub fn convert_dbus_layout_to_raw_node(
    id: i32,
    props: HashMap<String, zbus::OwnedVariant>,
    children: Vec<zbus::OwnedVariant>,
) -> RawMenuNode {
    convert_dbus_layout_to_raw_node_bounded(id, props, children, 0)
}

fn convert_dbus_layout_to_raw_node_bounded(
    id: i32,
    props: HashMap<String, zbus::OwnedVariant>,
    children: Vec<zbus::OwnedVariant>,
    depth: usize,
) -> RawMenuNode {
    let mut raw_props = HashMap::with_capacity(props.len());
    for (k, v) in props {
        raw_props.insert(k, owned_variant_to_raw_value(&v));
    }

    let mut raw_children = Vec::with_capacity(children.len());
    if depth <= MAX_MENU_DEPTH + 1 {
        for child in children {
            let child_variant = unwrap_variant(&child);
            if let Variant::Structure(st) = child_variant {
                let fields = st.fields();
                if fields.len() >= 3 {
                    let id_field = unwrap_variant(&fields[0]);
                    let cid = match id_field {
                        Variant::I32(i) => *i,
                        Variant::U32(u) => (*u).cast_signed(),
                        _ => -1,
                    };
                    let mut cprops = HashMap::new();
                    let dict_field = unwrap_variant(&fields[1]);
                    if let Variant::Dict(dict) = dict_field {
                        for (k, v) in dict.iter() {
                            if let Variant::Str(ks) = unwrap_variant(k) {
                                if let Ok(owned) = zbus::OwnedVariant::try_from(v.clone()) {
                                    cprops.insert(ks.as_str().to_string(), owned);
                                }
                            }
                        }
                    }
                    let mut sub_children = Vec::new();
                    let arr_field = unwrap_variant(&fields[2]);
                    if let Variant::Array(arr) = arr_field {
                        for item in arr.iter() {
                            if let Ok(owned) = zbus::OwnedVariant::try_from(item.clone()) {
                                sub_children.push(owned);
                            }
                        }
                    }
                    raw_children.push(convert_dbus_layout_to_raw_node_bounded(
                        cid,
                        cprops,
                        sub_children,
                        depth + 1,
                    ));
                }
            }
        }
    }

    RawMenuNode {
        id,
        properties: raw_props,
        children: raw_children,
    }
}

/// Converts a validated domain menu item into an IPC DTO.
#[must_use]
pub fn validated_item_to_dto(item: &ValidatedMenuItem) -> MenuItemDto {
    MenuItemDto {
        id: item.id,
        parent: item.parent,
        label: item.label.clone(),
        enabled: item.enabled,
        visible: item.visible,
        kind: item.kind.as_u32(),
        toggle: item.toggle.as_u32(),
        toggle_state: item.toggle_state.as_i32(),
        icon_name: item.icon_name.clone(),
        disposition: item.disposition.as_u32(),
        shortcut: item.shortcut.clone(),
        has_submenu: item.has_submenu,
    }
}

/// Manages fetching, caching, rate limiting, and event validation for client menus.
#[allow(clippy::type_complexity)]
pub struct MenuManager {
    /// Cached layout entries keyed by canonical item ID.
    cache: RwLock<HashMap<String, MenuCacheEntry>>,
    /// Active in-flight fetch broadcasters keyed by canonical item ID.
    in_flight: Mutex<HashMap<String, broadcast::Sender<Result<(u32, Vec<MenuItemDto>), String>>>>,
    /// Bounded concurrency semaphore for layout fetches.
    semaphore: Arc<Semaphore>,
    /// Token-bucket refresh rate limiter per item.
    rate_limiter: Mutex<RefreshRateLimiter>,
    /// Signal listener cancellation handles.
    listener_cancellers: Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>,
}

impl MenuManager {
    /// Creates a new menu manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            in_flight: Mutex::new(HashMap::new()),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES)),
            rate_limiter: Mutex::new(RefreshRateLimiter::new()),
            listener_cancellers: Mutex::new(HashMap::new()),
        }
    }

    /// Fetches the menu layout for `item_id`, returning cached data if available or
    /// coalescing concurrent requests onto a single bounded network fetch.
    ///
    /// # Errors
    /// Returns `zbus::fdo::Error` on timeout, D-Bus communication failure, or validation errors.
    pub async fn get_menu(
        &self,
        conn: &Connection,
        bus_name: &str,
        menu_path: &str,
        item_id: &str,
    ) -> zbus::fdo::Result<(u32, Vec<MenuItemDto>)> {
        // 1. Check cache first (instant hit)
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(item_id) {
                return Ok((entry.revision, entry.dtos.clone()));
            }
        }

        // 2. Coalesce concurrent fetches for the same item
        let (rx, is_initiator) = {
            let mut in_flight = self.in_flight.lock().await;
            if let Some(tx) = in_flight.get(item_id) {
                (Some(tx.subscribe()), false)
            } else {
                let (tx, rx) = broadcast::channel(1);
                in_flight.insert(item_id.to_string(), tx);
                (Some(rx), true)
            }
        };

        if !is_initiator {
            if let Some(mut subscriber) = rx {
                match subscriber.recv().await {
                    Ok(Ok(res)) => return Ok(res),
                    Ok(Err(e)) => return Err(zbus::fdo::Error::Failed(e)),
                    Err(_) => {
                        return Err(zbus::fdo::Error::Failed(
                            "In-flight menu fetch was aborted".to_string(),
                        ))
                    }
                }
            }
        }

        // 3. Initiator performs the bounded fetch
        let res = self
            .fetch_remote_layout(conn, bus_name, menu_path, item_id)
            .await;

        // Broadcast result to all waiting subscribers and remove in-flight tracker
        let tx = {
            let mut in_flight = self.in_flight.lock().await;
            in_flight.remove(item_id)
        };

        if let Some(tx) = tx {
            let broadcast_msg = match &res {
                Ok(data) => Ok(data.clone()),
                Err(e) => Err(format!("{e}")),
            };
            let _ = tx.send(broadcast_msg);
        }

        res
    }

    /// Performs the remote D-Bus call to `GetLayout`, parsing and caching the layout.
    async fn fetch_remote_layout(
        &self,
        conn: &Connection,
        bus_name: &str,
        menu_path: &str,
        item_id: &str,
    ) -> zbus::fdo::Result<(u32, Vec<MenuItemDto>)> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Semaphore error: {e}")))?;

        let proxy = DBusMenuProxy::builder(conn)
            .destination(bus_name)
            .path(menu_path)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| {
                zbus::fdo::Error::Failed(format!("Failed to build DBusMenu proxy: {e}"))
            })?;

        let depth_arg = i32::try_from(MAX_MENU_DEPTH).unwrap_or(6);
        let call_fut = proxy.get_layout(0, depth_arg, &[]);
        let (revision, (root_id, props, children)) =
            tokio::time::timeout(Duration::from_secs(MENU_TIMEOUT_SECS), call_fut)
                .await
                .map_err(|_| {
                    zbus::fdo::Error::Timeout("Timed out calling GetLayout after 2s".to_string())
                })?
                .map_err(|e| zbus::fdo::Error::Failed(format!("GetLayout error: {e}")))?;

        let raw_root = convert_dbus_layout_to_raw_node(root_id, props, children);
        let layout = parse_menu_layout(revision, &raw_root)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Layout validation failed: {e}")))?;

        let dtos: Vec<MenuItemDto> = layout.items.iter().map(validated_item_to_dto).collect();

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                item_id.to_string(),
                MenuCacheEntry {
                    revision,
                    layout,
                    dtos: dtos.clone(),
                },
            );
        }

        Ok((revision, dtos))
    }

    /// Informs the remote menu that item `menu_item_id` is about to be displayed.
    /// If the item requests an update (`need_update == true`), triggers a bounded re-fetch.
    ///
    /// # Errors
    /// Returns `zbus::fdo::Error` on timeout or D-Bus communication failure.
    pub async fn menu_about_to_show(
        &self,
        conn: &Connection,
        bus_name: &str,
        menu_path: &str,
        item_id: &str,
        menu_item_id: i32,
    ) -> zbus::fdo::Result<bool> {
        let proxy = DBusMenuProxy::builder(conn)
            .destination(bus_name)
            .path(menu_path)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| {
                zbus::fdo::Error::Failed(format!("Failed to build DBusMenu proxy: {e}"))
            })?;

        let call_fut = proxy.about_to_show(menu_item_id);
        let need_update = tokio::time::timeout(Duration::from_secs(MENU_TIMEOUT_SECS), call_fut)
            .await
            .map_err(|_| {
                zbus::fdo::Error::Timeout("Timed out calling AboutToShow after 2s".to_string())
            })?
            .map_err(|e| zbus::fdo::Error::Failed(format!("AboutToShow error: {e}")))?;

        if need_update {
            let _ = self
                .fetch_remote_layout(conn, bus_name, menu_path, item_id)
                .await;
        }

        Ok(need_update)
    }

    /// Dispatches a user action event to the remote dbusmenu service after validating
    /// it against the current validated layout.
    ///
    /// # Errors
    /// Returns `zbus::fdo::Error` on invalid event arguments, item not enabled/found, timeout, or D-Bus failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn menu_event(
        &self,
        conn: &Connection,
        bus_name: &str,
        menu_path: &str,
        item_id: &str,
        menu_item_id: i32,
        event: &str,
        revision: u32,
    ) -> zbus::fdo::Result<()> {
        // Validate against current cached layout and revision
        {
            let cache = self.cache.read().await;
            let entry = cache.get(item_id).ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs(format!("No active menu loaded for item '{item_id}'"))
            })?;

            if entry.revision != revision {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Stale menu revision: expected {}, got {revision}",
                    entry.revision
                )));
            }

            validate_menu_event(&entry.layout, menu_item_id, event)
                .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Invalid menu event: {e}")))?;
        }

        let proxy = DBusMenuProxy::builder(conn)
            .destination(bus_name)
            .path(menu_path)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| {
                zbus::fdo::Error::Failed(format!("Failed to build DBusMenu proxy: {e}"))
            })?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u32::try_from(d.as_secs()).unwrap_or(0));

        let call_fut = proxy.event(menu_item_id, event, &zbus::Variant::I32(0), timestamp);

        tokio::time::timeout(Duration::from_secs(MENU_TIMEOUT_SECS), call_fut)
            .await
            .map_err(|_| zbus::fdo::Error::Timeout("Timed out calling Event after 2s".to_string()))?
            .map_err(|e| zbus::fdo::Error::Failed(format!("Event error: {e}")))?;

        Ok(())
    }

    /// Evicts an item from cache and cancels active background listeners when removed.
    pub async fn on_item_removed(&self, item_id: &str) {
        self.cache.write().await.remove(item_id);
        if let Some(canceller) = self.listener_cancellers.lock().await.remove(item_id) {
            let _ = canceller.send(());
        }
    }

    /// Spawns a background listener for `LayoutUpdated` and `ItemsPropertiesUpdated` signals.
    pub async fn register_signal_listener(
        self: &Arc<Self>,
        conn: &Connection,
        key: &ItemKey,
        menu_path: &str,
        tray1_emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
    ) {
        let item_id = key.as_id_string();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();
        self.listener_cancellers
            .lock()
            .await
            .insert(item_id.clone(), cancel_tx);

        let bus_name = key.bus_name.clone();
        let menu_path = menu_path.to_string();
        let conn_clone = conn.clone();
        let self_clone = Arc::clone(self);

        tokio::spawn(async move {
            let proxy_res = DBusMenuProxy::builder(&conn_clone)
                .destination(bus_name.as_str())
                .path(menu_path.as_str())
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await;

            let proxy = match proxy_res {
                Ok(p) => p,
                Err(e) => {
                    warn!("Failed to build DBusMenu proxy for signals on '{item_id}': {e}");
                    return;
                }
            };

            let mut layout_stream = match proxy.receive_layout_updated().await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to receive LayoutUpdated for '{item_id}': {e}");
                    return;
                }
            };

            let mut props_stream = match proxy.receive_items_properties_updated().await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to receive ItemsPropertiesUpdated for '{item_id}': {e}");
                    return;
                }
            };

            loop {
                tokio::select! {
                    _ = &mut cancel_rx => {
                        debug!("Menu signal listener for '{item_id}' cancelled");
                        break;
                    }
                    Some(signal) = futures_util::StreamExt::next(&mut layout_stream) => {
                        if signal.args().is_ok() {
                            // Check rate limit
                            let allowed = self_clone.rate_limiter.lock().await.check_and_consume(&item_id, Instant::now());
                            if allowed.is_err() {
                                debug!("Dropping LayoutUpdated for '{item_id}' due to rate limit");
                                continue;
                            }

                            // Re-fetch layout
                            if let Ok((new_rev, _)) = self_clone.fetch_remote_layout(&conn_clone, &bus_name, &menu_path, &item_id).await {
                                if let Some(emitter) = tray1_emitter.lock().await.as_ref() {
                                    let _ = crate::services::tray::TrayServer::menu_changed(emitter, &item_id, new_rev).await;
                                }
                            }
                        }
                    }
                    Some(_signal) = futures_util::StreamExt::next(&mut props_stream) => {
                        let allowed = self_clone.rate_limiter.lock().await.check_and_consume(&item_id, Instant::now());
                        if allowed.is_err() {
                            debug!("Dropping ItemsPropertiesUpdated for '{item_id}' due to rate limit");
                            continue;
                        }

                        if let Ok((new_rev, _)) = self_clone.fetch_remote_layout(&conn_clone, &bus_name, &menu_path, &item_id).await {
                            if let Some(emitter) = tray1_emitter.lock().await.as_ref() {
                                let _ = crate::services::tray::TrayServer::menu_changed(emitter, &item_id, new_rev).await;
                            }
                        }
                    }
                }
            }
        });
    }
}

impl Default for MenuManager {
    fn default() -> Self {
        Self::new()
    }
}
