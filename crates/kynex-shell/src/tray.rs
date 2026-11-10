//! System Tray list model exposing StatusNotifierItem icons to QML via `cxx-qt`.
#![allow(
    clippy::chunks_exact_to_as_chunks,
    clippy::missing_errors_doc,
    clippy::too_many_arguments,
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls,
    clippy::type_complexity,
    clippy::too_many_lines,
    clippy::borrow_as_ptr,
    clippy::bool_assert_comparison
)]

use core::pin::Pin;
use std::collections::HashMap;
use std::sync::RwLock;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use futures_util::StreamExt;
use kynex_ipc::tray::{
    StatusNotifierWatcherProxy, Tray1Proxy, TrayItemInfo, STATUS_NOTIFIER_WATCHER_DESTINATION_FDO,
    STATUS_NOTIFIER_WATCHER_DESTINATION_KDE, STATUS_NOTIFIER_WATCHER_PATH, TRAY_INTERFACE,
    TRAY_PATH,
};
use kynex_ipc::DAEMON_DESTINATION;
use kynex_shell_core::tray::{TrayUiItem, TrayUiState};
use tracing::{info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::{MatchRule, MessageStream};

/// Role ID for item unique identifier.
pub const ROLE_ID: i32 = 257;
/// Role ID for item title.
pub const ROLE_TITLE: i32 = 258;
/// Role ID for item status (`"Passive"`, `"Active"`, `"NeedsAttention"`).
pub const ROLE_STATUS: i32 = 259;
/// Role ID for icon name.
pub const ROLE_ICON_NAME: i32 = 260;
/// Role ID for whether an in-memory pixmap is present.
pub const ROLE_HAS_PIXMAP: i32 = 261;
/// Role ID for pixmap identifier (used with `image://kynex-tray/<id>`).
pub const ROLE_PIXMAP_ID: i32 = 262;
/// Role ID for tooltip headline.
pub const ROLE_TOOLTIP_TITLE: i32 = 263;
/// Role ID for tooltip body text.
pub const ROLE_TOOLTIP_TEXT: i32 = 264;
/// Role ID for whether the item requests attention.
pub const ROLE_IS_ATTENTION: i32 = 265;
/// Role ID for single-character fallback letter badge.
pub const ROLE_LETTER_BADGE: i32 = 266;
/// Role ID for whether item is menu-only.
pub const ROLE_IS_MENU: i32 = 267;
/// Role ID for associated window ID.
pub const ROLE_WINDOW_ID: i32 = 268;

static TRAY_PIXMAP_CACHE: RwLock<Option<HashMap<String, (i32, i32, Vec<u8>)>>> = RwLock::new(None);

/// Updates or inserts a pixmap in the global tray pixmap cache.
pub fn update_pixmap_cache(id: &str, width: i32, height: i32, pixmap: &[u8]) {
    if let Ok(mut lock) = TRAY_PIXMAP_CACHE.write() {
        let map = lock.get_or_insert_with(HashMap::new);
        if pixmap.is_empty() || width <= 0 || height <= 0 {
            map.remove(id);
        } else {
            map.insert(id.to_string(), (width, height, pixmap.to_vec()));
        }
    }
}

/// Removes a pixmap from the global tray pixmap cache.
pub fn remove_from_pixmap_cache(id: &str) {
    if let Ok(mut lock) = TRAY_PIXMAP_CACHE.write() {
        if let Some(map) = lock.as_mut() {
            map.remove(id);
        }
    }
}

/// Clears the global tray pixmap cache.
pub fn clear_pixmap_cache() {
    if let Ok(mut lock) = TRAY_PIXMAP_CACHE.write() {
        if let Some(map) = lock.as_mut() {
            map.clear();
        }
    }
}

/// Copies cached tray icon pixmap bytes for a given item id into `out_buf`.
///
/// # Safety
/// `id` must be a valid null-terminated C string.
/// `out_width` and `out_height` must point to writable `i32` memory.
/// `out_buf` must point to a writable buffer of at least `max_buf_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn kynex_get_tray_pixmap(
    id: *const std::ffi::c_char,
    out_width: *mut i32,
    out_height: *mut i32,
    out_buf: *mut u8,
    max_buf_len: usize,
) -> usize {
    if id.is_null()
        || out_width.is_null()
        || out_height.is_null()
        || out_buf.is_null()
        || max_buf_len == 0
    {
        return 0;
    }

    let Ok(c_str) = std::ffi::CStr::from_ptr(id).to_str() else {
        return 0;
    };

    if let Ok(lock) = TRAY_PIXMAP_CACHE.read() {
        if let Some(map) = lock.as_ref() {
            if let Some((width, height, data)) = map.get(c_str) {
                *out_width = *width;
                *out_height = *height;
                let copy_len = data.len().min(max_buf_len);
                std::ptr::copy_nonoverlapping(data.as_ptr(), out_buf, copy_len);
                return copy_len;
            }
        }
    }

    0
}

/// Converts a tray item and role ID into a `QVariant`.
#[must_use]
pub fn tray_item_role_variant(item: &TrayUiItem, role: i32) -> QVariant {
    match role {
        ROLE_ID => QVariant::from(&QString::from(&item.id)),
        ROLE_TITLE => QVariant::from(&QString::from(&item.title)),
        ROLE_STATUS => QVariant::from(&QString::from(&item.status)),
        ROLE_ICON_NAME => QVariant::from(&QString::from(&item.icon_name)),
        ROLE_HAS_PIXMAP => QVariant::from(&item.has_pixmap),
        ROLE_PIXMAP_ID => QVariant::from(&QString::from(&item.pixmap_id)),
        ROLE_TOOLTIP_TITLE => QVariant::from(&QString::from(&item.tooltip_title)),
        ROLE_TOOLTIP_TEXT => QVariant::from(&QString::from(&item.tooltip_text)),
        ROLE_IS_ATTENTION => QVariant::from(&item.is_attention),
        ROLE_LETTER_BADGE => QVariant::from(&QString::from(&item.letter_badge)),
        ROLE_IS_MENU => QVariant::from(&item.is_menu),
        ROLE_WINDOW_ID => QVariant::from(&item.window_id),
        _ => QVariant::default(),
    }
}

/// Constructs the role names hash map mapping role IDs to QML property strings.
#[must_use]
pub fn create_tray_role_names() -> QHash<QHashPair_i32_QByteArray> {
    let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
    roles.insert(ROLE_ID, QByteArray::from("id"));
    roles.insert(ROLE_TITLE, QByteArray::from("title"));
    roles.insert(ROLE_STATUS, QByteArray::from("status"));
    roles.insert(ROLE_ICON_NAME, QByteArray::from("iconName"));
    roles.insert(ROLE_HAS_PIXMAP, QByteArray::from("hasPixmap"));
    roles.insert(ROLE_PIXMAP_ID, QByteArray::from("pixmapId"));
    roles.insert(ROLE_TOOLTIP_TITLE, QByteArray::from("tooltipTitle"));
    roles.insert(ROLE_TOOLTIP_TEXT, QByteArray::from("tooltipText"));
    roles.insert(ROLE_IS_ATTENTION, QByteArray::from("isAttention"));
    roles.insert(ROLE_LETTER_BADGE, QByteArray::from("letterBadge"));
    roles.insert(ROLE_IS_MENU, QByteArray::from("isMenu"));
    roles.insert(ROLE_WINDOW_ID, QByteArray::from("windowId"));
    roles
}

/// CXX-Qt bridge module for `TrayModel`.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qmodelindex.h");
        /// Model index type.
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        /// Variant type.
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qhash_i32_QByteArray.h");
        /// Hash map of integer to byte array.
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qstring.h");
        /// QString type.
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        /// Qt QAbstractListModel base class.
        #[qobject]
        type QAbstractListModel;
    }

    extern "RustQt" {
        /// System tray list model for QML presentation.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        #[qproperty(bool, has_items)]
        type TrayModel = super::TrayModelRust;

        /// Initializes background D-Bus listeners and registers as StatusNotifierHost.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Returns the number of items in the model.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TrayModel, parent: &QModelIndex) -> i32;

        /// Returns role data for a given model index and role.
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TrayModel, index: &QModelIndex, role: i32) -> QVariant;

        /// Returns role names mapping for QML property bindings.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TrayModel) -> QHash_i32_QByteArray;

        /// Invokes primary activation on target item.
        #[qinvokable]
        fn activate(self: Pin<&mut Self>, id: &QString, x: i32, y: i32);

        /// Invokes secondary activation on target item.
        #[qinvokable]
        #[cxx_name = "secondaryActivate"]
        fn secondary_activate(self: Pin<&mut Self>, id: &QString, x: i32, y: i32);

        /// Invokes context menu on target item with window coordinates.
        #[qinvokable]
        #[cxx_name = "contextMenu"]
        fn context_menu(self: Pin<&mut Self>, id: &QString, x: i32, y: i32);

        /// Invokes scroll action on target item.
        #[qinvokable]
        fn scroll(self: Pin<&mut Self>, id: &QString, delta: i32, orientation: &QString);

        /// Records the first frame rendered after a tray change.
        #[qinvokable]
        #[cxx_name = "recordFirstFrame"]
        fn record_first_frame(self: Pin<&mut Self>);
    }

    unsafe extern "RustQt" {
        /// Inherited beginResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut TrayModel>);

        /// Inherited endResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut TrayModel>);
    }

    impl cxx_qt::Threading for TrayModel {}
}

/// Backing data for `TrayModel`.
#[derive(Default)]
pub struct TrayModelRust {
    pub(crate) state: TrayUiState,
    pub(crate) count: i32,
    pub(crate) has_items: bool,
    pub(crate) first_frame_pending: bool,
    pub(crate) last_event_time: Option<std::time::Instant>,
}

impl TrayModelRust {
    /// Sets state directly.
    pub fn set_state(&mut self, state: TrayUiState) {
        self.count = i32::try_from(state.items.len()).unwrap_or(0);
        self.has_items = self.count > 0;
        self.state = state;
    }

    /// Accesses item at row.
    #[must_use]
    pub fn item_at(&self, row: usize) -> Option<&TrayUiItem> {
        self.state.items.get(row)
    }

    /// Returns row count for the model.
    #[must_use]
    pub fn row_count(&self) -> i32 {
        self.count
    }

    /// Returns role data for a given row and role.
    #[must_use]
    pub fn data(&self, row: i32, role: i32) -> QVariant {
        let Ok(row_idx) = usize::try_from(row) else {
            return QVariant::default();
        };

        let items = &self.state.items;
        if row_idx >= items.len() {
            return QVariant::default();
        }

        tray_item_role_variant(&items[row_idx], role)
    }

    /// Records the first frame rendered after a tray item was updated or added.
    pub fn record_first_frame(&mut self) -> Option<f64> {
        if self.first_frame_pending {
            self.first_frame_pending = false;
            let frame_ns = crate::shim::ffi::kynex_steady_clock_ns();
            let elapsed_ms = self
                .last_event_time
                .map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0);
            tracing::info!(
                target: "kynex_perf",
                event = "tray_first_frame_swapped",
                frame_ns = frame_ns,
                latency_ms = elapsed_ms,
                "Tray first frame swapped: frame_ns={frame_ns} shell_latency_ms={elapsed_ms:.2}"
            );
            Some(elapsed_ms)
        } else {
            None
        }
    }
}

impl qobject::TrayModel {
    /// Initializes background listeners connecting to `kynex-daemon`.
    pub fn init(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_tray_listener(qt_thread).await;
        });
    }

    /// Returns row count for the model.
    #[must_use]
    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().row_count()
    }

    /// Returns role data for a given index.
    #[must_use]
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        self.rust().data(index.row(), role)
    }

    /// Returns role names mapping for QML.
    #[must_use]
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        create_tray_role_names()
    }

    /// Invokes primary activation on target item.
    pub fn activate(self: Pin<&mut Self>, id: &QString, x: i32, y: i32) {
        let id_str = id.to_string();
        tokio::spawn(async move {
            let _ = send_activate_call(&id_str, x, y).await;
        });
    }

    /// Invokes secondary activation on target item.
    pub fn secondary_activate(self: Pin<&mut Self>, id: &QString, x: i32, y: i32) {
        let id_str = id.to_string();
        tokio::spawn(async move {
            let _ = send_secondary_activate_call(&id_str, x, y).await;
        });
    }

    /// Invokes context menu on target item with window coordinates.
    pub fn context_menu(self: Pin<&mut Self>, id: &QString, x: i32, y: i32) {
        let id_str = id.to_string();
        tokio::spawn(async move {
            let _ = send_context_menu_call(&id_str, x, y).await;
        });
    }

    /// Invokes scroll action on target item.
    pub fn scroll(self: Pin<&mut Self>, id: &QString, delta: i32, orientation: &QString) {
        let id_str = id.to_string();
        let orient_str = orientation.to_string();
        tokio::spawn(async move {
            let _ = send_scroll_call(&id_str, delta, &orient_str).await;
        });
    }

    /// Records the first frame rendered after a tray change.
    pub fn record_first_frame(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().record_first_frame();
    }
}

/// Sends Activate D-Bus call to daemon.
pub async fn send_activate_call(id: &str, x: i32, y: i32) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session().build().await?;
    let proxy = Tray1Proxy::new(&conn).await?;
    proxy.activate(id, x, y).await
}

/// Sends SecondaryActivate D-Bus call to daemon.
pub async fn send_secondary_activate_call(id: &str, x: i32, y: i32) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session().build().await?;
    let proxy = Tray1Proxy::new(&conn).await?;
    proxy.secondary_activate(id, x, y).await
}

/// Sends ContextMenu D-Bus call to daemon.
pub async fn send_context_menu_call(id: &str, x: i32, y: i32) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session().build().await?;
    let proxy = Tray1Proxy::new(&conn).await?;
    proxy.context_menu(id, x, y).await
}

/// Sends Scroll D-Bus call to daemon.
pub async fn send_scroll_call(id: &str, delta: i32, orientation: &str) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session().build().await?;
    let proxy = Tray1Proxy::new(&conn).await?;
    proxy.scroll(id, delta, orientation).await
}

/// Dispatches updated tray state to the Qt GUI thread.
fn dispatch_state(qt_thread: &cxx_qt::CxxQtThread<qobject::TrayModel>, state: TrayUiState) {
    let count = i32::try_from(state.items.len()).unwrap_or(0);
    let has_items = count > 0;
    let now = std::time::Instant::now();
    let _ = qt_thread.queue(move |mut qobj| {
        qobj.as_mut().begin_reset_model();
        let mut r = qobj.as_mut().rust_mut();
        r.state = state;
        if has_items {
            r.first_frame_pending = true;
            r.last_event_time = Some(now);
        }
        qobj.as_mut().set_count(count);
        qobj.as_mut().set_has_items(has_items);
        qobj.as_mut().end_reset_model();
    });
}

/// Registers the shell as a StatusNotifierHost on available watchers.
pub async fn register_status_notifier_host(conn: &zbus::Connection) {
    if let Ok(kde_watcher) = StatusNotifierWatcherProxy::builder(conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_KDE)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .build()
        .await
    {
        let _ = kde_watcher
            .register_status_notifier_host("kynex-shell")
            .await;
    }

    if let Ok(fdo_watcher) = StatusNotifierWatcherProxy::builder(conn)
        .destination(STATUS_NOTIFIER_WATCHER_DESTINATION_FDO)
        .path(STATUS_NOTIFIER_WATCHER_PATH)
        .build()
        .await
    {
        let _ = fdo_watcher
            .register_status_notifier_host("kynex-shell")
            .await;
    }
}

/// Initializes a match-rule signal stream on `TRAY_PATH`.
async fn init_signal_stream(conn: &zbus::Connection) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path(TRAY_PATH)
        .interface(TRAY_INTERFACE)
        .build()
        .ok()?;
    MessageStream::for_match_rule(rule, conn, None).await.ok()
}

/// Handles an incoming tray signal message, updating UI state and pixmap cache.
/// Returns whether the state was updated.
pub fn handle_tray_signal_message(state: &mut TrayUiState, msg: &zbus::Message) -> bool {
    let header = msg.header();
    let Some(member) = header.member() else {
        return false;
    };

    match member.as_str() {
        "ItemAdded" | "ItemChanged" => {
            if let Ok(info) = msg.body().deserialize::<TrayItemInfo>() {
                if !info.icon_pixmap.is_empty()
                    && info.icon_pixmap_width > 0
                    && info.icon_pixmap_height > 0
                {
                    update_pixmap_cache(
                        &info.id,
                        info.icon_pixmap_width,
                        info.icon_pixmap_height,
                        &info.icon_pixmap,
                    );
                } else {
                    remove_from_pixmap_cache(&info.id);
                }
                let item = TrayUiItem::from_ipc(&info);
                state.upsert(item);
                true
            } else {
                false
            }
        }
        "ItemRemoved" => {
            if let Ok(id) = msg.body().deserialize::<String>() {
                remove_from_pixmap_cache(&id);
                state.remove(&id).is_some()
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Handles a NameOwnerChanged event for the daemon destination.
/// Returns whether the daemon disconnected and state should be reset.
pub fn handle_name_owner_changed_event(state: &mut TrayUiState, new_owner: Option<&str>) -> bool {
    if new_owner.as_ref().is_none_or(|o| o.is_empty()) {
        info!("kynex-daemon left the bus; resetting tray state");
        state.items.clear();
        clear_pixmap_cache();
        true
    } else {
        false
    }
}

/// Event loop for tray D-Bus signals and name changes.
pub async fn run_tray_listener_loop<F>(
    conn: &zbus::Connection,
    mut on_state_change: F,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) where
    F: FnMut(TrayUiState),
{
    let dbus_proxy = match DBusProxy::new(conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("TrayModel failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = match dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
    {
        Ok(s) => Some(s),
        Err(err) => {
            warn!("TrayModel failed to register for NameOwnerChanged: {err}");
            None
        }
    };

    register_status_notifier_host(conn).await;

    let mut state = TrayUiState::default();
    if let Ok(proxy) = Tray1Proxy::new(conn).await {
        if let Ok(items) = proxy.get_items().await {
            for info in &items {
                if !info.icon_pixmap.is_empty()
                    && info.icon_pixmap_width > 0
                    && info.icon_pixmap_height > 0
                {
                    update_pixmap_cache(
                        &info.id,
                        info.icon_pixmap_width,
                        info.icon_pixmap_height,
                        &info.icon_pixmap,
                    );
                }
                state.upsert(TrayUiItem::from_ipc(info));
            }
            on_state_change(state.clone());
        }
    }

    let mut signal_stream = init_signal_stream(conn).await;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }

            Some(msg) = async {
                if let Some(ref mut s) = signal_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(msg) = msg {
                    if handle_tray_signal_message(&mut state, &msg) {
                        on_state_change(state.clone());
                    }
                }
            }

            Some(name_event) = async {
                if let Some(ref mut s) = name_owner_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = name_event.args() {
                    let new_owner = args.new_owner();
                    let new_owner_str = new_owner.as_ref().map(|o| o.as_str());
                    if handle_name_owner_changed_event(&mut state, new_owner_str) {
                        on_state_change(state.clone());
                        signal_stream = None;
                    } else {
                        info!("kynex-daemon registered on bus; reconnecting tray stream");
                        register_status_notifier_host(conn).await;
                        if let Ok(proxy) = Tray1Proxy::new(conn).await {
                            if let Ok(items) = proxy.get_items().await {
                                state.items.clear();
                                clear_pixmap_cache();
                                for info in &items {
                                    if !info.icon_pixmap.is_empty()
                                        && info.icon_pixmap_width > 0
                                        && info.icon_pixmap_height > 0
                                    {
                                        update_pixmap_cache(
                                            &info.id,
                                            info.icon_pixmap_width,
                                            info.icon_pixmap_height,
                                            &info.icon_pixmap,
                                        );
                                    }
                                    state.upsert(TrayUiItem::from_ipc(info));
                                }
                                on_state_change(state.clone());
                            }
                        }
                        signal_stream = init_signal_stream(conn).await;
                    }
                }
            }
        }
    }
}

/// Background asynchronous listener connecting to `org.kynexos.Daemon.Tray1`.
pub async fn run_tray_listener(qt_thread: cxx_qt::CxxQtThread<qobject::TrayModel>) {
    let session_conn = match zbus::connection::Builder::session().build().await {
        Ok(c) => c,
        Err(err) => {
            warn!("TrayModel failed to connect to session bus: {err}");
            return;
        }
    };

    let (_tx, rx) = tokio::sync::watch::channel(false);
    run_tray_listener_loop(
        &session_conn,
        move |state| {
            dispatch_state(&qt_thread, state);
        },
        rx,
    )
    .await;
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::manual_string_new
)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_model_rust_defaults() {
        let mut rust = TrayModelRust::default();
        assert_eq!(rust.row_count(), 0);
        assert!(!rust.has_items);
        assert!(rust.item_at(0).is_none());

        let mut state = TrayUiState::default();
        state.upsert(TrayUiItem {
            id: ":1.42/StatusNotifierItem".to_string(),
            title: "Test".to_string(),
            status: "Active".to_string(),
            icon_name: "test-icon".to_string(),
            has_pixmap: false,
            pixmap_id: ":1.42/StatusNotifierItem".to_string(),
            tooltip_title: "Title".to_string(),
            tooltip_text: "Desc".to_string(),
            is_attention: false,
            letter_badge: "T".to_string(),
            is_menu: false,
            window_id: 0,
            icon_pixmap: Vec::new(),
            icon_pixmap_width: 0,
            icon_pixmap_height: 0,
        });

        rust.set_state(state);
        assert_eq!(rust.row_count(), 1);
        assert!(rust.has_items);
        assert!(rust.item_at(0).is_some());
        assert_eq!(rust.item_at(0).unwrap().id, ":1.42/StatusNotifierItem");

        rust.first_frame_pending = true;
        rust.last_event_time = Some(std::time::Instant::now());
        let latency = rust.record_first_frame();
        assert!(latency.is_some());
        assert!(!rust.first_frame_pending);
        assert!(rust.record_first_frame().is_none());
    }

    #[test]
    fn test_tray_item_role_variant_and_names() {
        let item = TrayUiItem {
            id: "item1".to_string(),
            title: "Item One".to_string(),
            status: "NeedsAttention".to_string(),
            icon_name: "icon1".to_string(),
            has_pixmap: true,
            pixmap_id: "item1".to_string(),
            tooltip_title: "Tip Title".to_string(),
            tooltip_text: "Tip Text".to_string(),
            is_attention: true,
            letter_badge: "I".to_string(),
            is_menu: false,
            window_id: 1234,
            icon_pixmap: vec![0; 16],
            icon_pixmap_width: 2,
            icon_pixmap_height: 2,
        };

        let role_names = create_tray_role_names();
        assert_eq!(role_names.len(), 12);
        assert_eq!(role_names.get(&ROLE_ID).unwrap().to_string(), "id");
        assert_eq!(role_names.get(&ROLE_TITLE).unwrap().to_string(), "title");
        assert_eq!(role_names.get(&ROLE_STATUS).unwrap().to_string(), "status");
        assert_eq!(
            role_names.get(&ROLE_ICON_NAME).unwrap().to_string(),
            "iconName"
        );
        assert_eq!(
            role_names.get(&ROLE_HAS_PIXMAP).unwrap().to_string(),
            "hasPixmap"
        );
        assert_eq!(
            role_names.get(&ROLE_PIXMAP_ID).unwrap().to_string(),
            "pixmapId"
        );
        assert_eq!(
            role_names.get(&ROLE_TOOLTIP_TITLE).unwrap().to_string(),
            "tooltipTitle"
        );
        assert_eq!(
            role_names.get(&ROLE_TOOLTIP_TEXT).unwrap().to_string(),
            "tooltipText"
        );
        assert_eq!(
            role_names.get(&ROLE_IS_ATTENTION).unwrap().to_string(),
            "isAttention"
        );
        assert_eq!(
            role_names.get(&ROLE_LETTER_BADGE).unwrap().to_string(),
            "letterBadge"
        );
        assert_eq!(role_names.get(&ROLE_IS_MENU).unwrap().to_string(), "isMenu");
        assert_eq!(
            role_names.get(&ROLE_WINDOW_ID).unwrap().to_string(),
            "windowId"
        );

        let v_id = tray_item_role_variant(&item, ROLE_ID);
        assert_eq!(v_id.value::<QString>().unwrap().to_string(), "item1");

        let v_att = tray_item_role_variant(&item, ROLE_IS_ATTENTION);
        assert_eq!(v_att.value::<bool>().unwrap(), true);

        let v_win = tray_item_role_variant(&item, ROLE_WINDOW_ID);
        assert_eq!(v_win.value::<i32>().unwrap(), 1234);

        let v_invalid = tray_item_role_variant(&item, 9999);
        assert!(v_invalid.is_null());
    }

    #[test]
    fn test_pixmap_cache_and_ffi() {
        clear_pixmap_cache();
        let id_str = std::ffi::CString::new("test-item").unwrap();
        let mut width: i32 = 0;
        let mut height: i32 = 0;
        let mut buffer = [0u8; 32];

        // Empty cache lookup
        unsafe {
            let copied = kynex_get_tray_pixmap(
                id_str.as_ptr(),
                &mut width,
                &mut height,
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            assert_eq!(copied, 0);
        }

        // Insert pixmap
        let sample_pixmap = vec![1, 2, 3, 4, 5, 6, 7, 8];
        update_pixmap_cache("test-item", 1, 2, &sample_pixmap);

        unsafe {
            let copied = kynex_get_tray_pixmap(
                id_str.as_ptr(),
                &mut width,
                &mut height,
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            assert_eq!(copied, 8);
            assert_eq!(width, 1);
            assert_eq!(height, 2);
            assert_eq!(&buffer[..8], &sample_pixmap[..]);
        }

        // Safety check null pointers
        unsafe {
            assert_eq!(
                kynex_get_tray_pixmap(
                    std::ptr::null(),
                    &mut width,
                    &mut height,
                    buffer.as_mut_ptr(),
                    buffer.len()
                ),
                0
            );
            assert_eq!(
                kynex_get_tray_pixmap(
                    id_str.as_ptr(),
                    std::ptr::null_mut(),
                    &mut height,
                    buffer.as_mut_ptr(),
                    buffer.len()
                ),
                0
            );
            assert_eq!(
                kynex_get_tray_pixmap(
                    id_str.as_ptr(),
                    &mut width,
                    &mut height,
                    std::ptr::null_mut(),
                    buffer.len()
                ),
                0
            );
            assert_eq!(
                kynex_get_tray_pixmap(
                    id_str.as_ptr(),
                    &mut width,
                    &mut height,
                    buffer.as_mut_ptr(),
                    0
                ),
                0
            );
        }

        // Removal
        remove_from_pixmap_cache("test-item");
        unsafe {
            let copied = kynex_get_tray_pixmap(
                id_str.as_ptr(),
                &mut width,
                &mut height,
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            assert_eq!(copied, 0);
        }
    }

    #[test]
    fn test_handle_name_owner_changed_event() {
        let mut state = TrayUiState::default();
        state.upsert(TrayUiItem {
            id: "id1".to_string(),
            title: "T".to_string(),
            status: "Active".to_string(),
            icon_name: "i".to_string(),
            has_pixmap: false,
            pixmap_id: "id1".to_string(),
            tooltip_title: "".to_string(),
            tooltip_text: "".to_string(),
            is_attention: false,
            letter_badge: "T".to_string(),
            is_menu: false,
            window_id: 0,
            icon_pixmap: Vec::new(),
            icon_pixmap_width: 0,
            icon_pixmap_height: 0,
        });

        // Daemon disconnected (empty new_owner)
        let reset = handle_name_owner_changed_event(&mut state, Some(""));
        assert!(reset);
        assert_eq!(state.items.len(), 0);

        // Daemon connected (non-empty new_owner)
        let reset = handle_name_owner_changed_event(&mut state, Some(":1.100"));
        assert!(!reset);
    }
}
