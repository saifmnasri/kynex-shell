//! Window and taskbar model exposing grouped application state to QML via `cxx-qt`.
//!
//! Subclasses `QAbstractListModel` to provide a reactive, zero-polling data model
//! for shell taskbars and window switchers adhering to M2.3:
//! - Pinned apps appear first in pin order, followed by unpinned running apps.
//! - Running indicators and active state tracking.
//! - Multi-window grouping with count badge and click cycling.
//! - Automatically reconnects to daemon services when restarted.

use core::pin::Pin;
use std::collections::HashMap;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use futures_util::StreamExt;
use kynex_ipc::apps::Apps1Proxy;
use kynex_ipc::pins::Pins1Proxy;
use kynex_ipc::windows::{WindowItem, Windows1Proxy};
use kynex_ipc::DAEMON_DESTINATION;
use kynex_shell_core::matching::AppCandidate;
use kynex_shell_core::taskbar::{TaskbarAction, TaskbarCore, TaskbarEntry};
use tokio::sync::Mutex;
use tracing::{info, trace, warn};
use zbus::fdo::DBusProxy;

/// Role ID for primary identifier (`app_id`).
pub const ROLE_ID: i32 = 257;
/// Role ID for display title.
pub const ROLE_TITLE: i32 = 258;
/// Role ID for application ID.
pub const ROLE_APP_ID: i32 = 259;
/// Role ID for minimized state.
pub const ROLE_MINIMIZED: i32 = 260;
/// Role ID for active focus state.
pub const ROLE_ACTIVE: i32 = 261;
/// Role ID for icon name or path.
pub const ROLE_ICON_NAME: i32 = 262;
/// Role ID for fallback letter badge.
pub const ROLE_LETTER_BADGE: i32 = 263;
/// Role ID for pinned status.
pub const ROLE_IS_PINNED: i32 = 264;
/// Role ID for running status.
pub const ROLE_IS_RUNNING: i32 = 265;
/// Role ID for open window count.
pub const ROLE_WINDOW_COUNT: i32 = 266;

/// CXX-Qt bridge module for `WindowListModel`.
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
        /// Taskbar and window list model for QML presentation.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        type WindowListModel = super::WindowListModelRust;

        /// Initializes background D-Bus listeners.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Returns the number of items in the model.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &WindowListModel, parent: &QModelIndex) -> i32;

        /// Returns role data for a given model index and role.
        #[qinvokable]
        #[cxx_override]
        fn data(self: &WindowListModel, index: &QModelIndex, role: i32) -> QVariant;

        /// Returns role names mapping for QML property bindings.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &WindowListModel) -> QHash_i32_QByteArray;

        /// Dispatches interaction click on taskbar item at `index`.
        #[qinvokable]
        #[cxx_name = "handleClick"]
        fn handle_click(self: Pin<&mut Self>, index: i32);

        /// Dispatches middle-click close on taskbar item at `index`.
        #[qinvokable]
        #[cxx_name = "handleMiddleClick"]
        fn handle_middle_click(self: Pin<&mut Self>, index: i32);

        /// Activates the window matching `id`.
        #[qinvokable]
        fn activate(self: Pin<&mut Self>, id: &QString);

        /// Minimizes the window matching `id`.
        #[qinvokable]
        fn minimize(self: Pin<&mut Self>, id: &QString);

        /// Restores the window matching `id` from minimized state.
        #[qinvokable]
        fn restore(self: Pin<&mut Self>, id: &QString);

        /// Closes the window matching `id`.
        #[qinvokable]
        fn close(self: Pin<&mut Self>, id: &QString);
    }

    unsafe extern "RustQt" {
        /// Inherited beginResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut WindowListModel>);

        /// Inherited endResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut WindowListModel>);
    }

    impl cxx_qt::Threading for WindowListModel {}
}

/// Backing data for `WindowListModel`.
#[derive(Default)]
pub struct WindowListModelRust {
    core: TaskbarCore,
    entries: Vec<TaskbarEntry>,
    count: i32,
}

impl WindowListModelRust {
    /// Returns role data for a given entry index.
    #[must_use]
    pub fn data(&self, row_idx: usize, role: i32) -> QVariant {
        let entries = &self.entries;
        if row_idx >= entries.len() {
            return QVariant::default();
        }

        let item = &entries[row_idx];
        match role {
            ROLE_ID | ROLE_APP_ID => QVariant::from(&QString::from(&item.app_id)),
            ROLE_TITLE => QVariant::from(&QString::from(&item.name)),
            ROLE_MINIMIZED => QVariant::from(&item.is_minimized),
            ROLE_ACTIVE => QVariant::from(&item.is_active),
            ROLE_ICON_NAME => QVariant::from(&QString::from(&item.icon_name)),
            ROLE_LETTER_BADGE => QVariant::from(&QString::from(&item.letter_badge)),
            ROLE_IS_PINNED => QVariant::from(&item.is_pinned),
            ROLE_IS_RUNNING => QVariant::from(&item.is_running),
            ROLE_WINDOW_COUNT => QVariant::from(&(i32::try_from(item.window_count).unwrap_or(0))),
            _ => QVariant::default(),
        }
    }
}

/// Returns role names mapping for QML.
#[must_use]
pub fn create_window_role_names() -> QHash<QHashPair_i32_QByteArray> {
    let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
    roles.insert(ROLE_ID, QByteArray::from("id"));
    roles.insert(ROLE_TITLE, QByteArray::from("title"));
    roles.insert(ROLE_APP_ID, QByteArray::from("appId"));
    roles.insert(ROLE_MINIMIZED, QByteArray::from("minimized"));
    roles.insert(ROLE_ACTIVE, QByteArray::from("active"));
    roles.insert(ROLE_ICON_NAME, QByteArray::from("iconName"));
    roles.insert(ROLE_LETTER_BADGE, QByteArray::from("letterBadge"));
    roles.insert(ROLE_IS_PINNED, QByteArray::from("isPinned"));
    roles.insert(ROLE_IS_RUNNING, QByteArray::from("isRunning"));
    roles.insert(ROLE_WINDOW_COUNT, QByteArray::from("windowCount"));
    roles
}

impl qobject::WindowListModel {
    /// Initializes background listeners connecting to `kynex-daemon`.
    pub fn init(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_taskbar_listener(qt_thread).await;
        });
    }

    /// Returns row count for the model.
    #[must_use]
    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().count
    }

    /// Returns role data for a given index.
    #[must_use]
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let row = index.row();
        let Ok(row_idx) = usize::try_from(row) else {
            return QVariant::default();
        };

        self.rust().data(row_idx, role)
    }

    /// Returns role names mapping for QML.
    #[must_use]
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        create_window_role_names()
    }

    /// Dispatches click interaction on taskbar item at `index`.
    pub fn handle_click(self: Pin<&mut Self>, index: i32) {
        let Ok(u) = usize::try_from(index) else {
            return;
        };
        let action = self.rust().core.handle_click(u);
        tokio::spawn(async move {
            if let Ok(conn) = zbus::connection::Builder::session().build().await {
                match action {
                    TaskbarAction::Launch { app_id } => {
                        if let Ok(proxy) = Apps1Proxy::new(&conn).await {
                            let _ = proxy.launch(&app_id).await;
                        }
                    }
                    TaskbarAction::Activate { window_id } => {
                        if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                            let _ = proxy.activate(&window_id).await;
                        }
                    }
                    TaskbarAction::Restore { window_id } => {
                        if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                            let _ = proxy.restore(&window_id).await;
                        }
                    }
                    TaskbarAction::Minimize { window_id } => {
                        if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                            let _ = proxy.minimize(&window_id).await;
                        }
                    }
                    TaskbarAction::Close { window_id } => {
                        if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                            let _ = proxy.close(&window_id).await;
                        }
                    }
                    TaskbarAction::None => {}
                }
            }
        });
    }

    /// Dispatches middle-click close on taskbar item at `index`.
    pub fn handle_middle_click(self: Pin<&mut Self>, index: i32) {
        let Ok(u) = usize::try_from(index) else {
            return;
        };
        let action = self.rust().core.handle_middle_click(u);
        tokio::spawn(async move {
            if let Ok(conn) = zbus::connection::Builder::session().build().await {
                if let TaskbarAction::Close { window_id } = action {
                    if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                        let _ = proxy.close(&window_id).await;
                    }
                }
            }
        });
    }

    /// Activates (focuses) the window matching `id`.
    pub fn activate(self: Pin<&mut Self>, id: &QString) {
        let id_str = id.to_string();
        tokio::spawn(async move {
            if let Ok(conn) = zbus::connection::Builder::session().build().await {
                if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                    let _ = proxy.activate(&id_str).await;
                }
            }
        });
    }

    /// Minimizes the window matching `id`.
    pub fn minimize(self: Pin<&mut Self>, id: &QString) {
        let id_str = id.to_string();
        tokio::spawn(async move {
            if let Ok(conn) = zbus::connection::Builder::session().build().await {
                if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                    let _ = proxy.minimize(&id_str).await;
                }
            }
        });
    }

    /// Restores the window matching `id` from minimized state.
    pub fn restore(self: Pin<&mut Self>, id: &QString) {
        let id_str = id.to_string();
        tokio::spawn(async move {
            if let Ok(conn) = zbus::connection::Builder::session().build().await {
                if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                    let _ = proxy.restore(&id_str).await;
                }
            }
        });
    }

    /// Closes the window matching `id`.
    pub fn close(self: Pin<&mut Self>, id: &QString) {
        let id_str = id.to_string();
        tokio::spawn(async move {
            if let Ok(conn) = zbus::connection::Builder::session().build().await {
                if let Ok(proxy) = Windows1Proxy::new(&conn).await {
                    let _ = proxy.close(&id_str).await;
                }
            }
        });
    }
}

/// Dispatches updated taskbar core and entries to the Qt main thread.
fn dispatch_taskbar(qt_thread: &cxx_qt::CxxQtThread<qobject::WindowListModel>, core: TaskbarCore) {
    let entries = core.entries().to_vec();
    let count = i32::try_from(entries.len()).unwrap_or(0);
    let running_count = entries.iter().filter(|e| e.is_running).count();
    let pinned_count = entries.iter().filter(|e| e.is_pinned).count();
    info!(
        "taskbar: {running_count} running windows, {pinned_count} pinned entries (total {count})"
    );
    let _ = qt_thread.queue(move |mut qobj| {
        qobj.as_mut().begin_reset_model();
        qobj.as_mut().rust_mut().core = core;
        qobj.as_mut().rust_mut().entries = entries;
        qobj.as_mut().rust_mut().count = count;
        qobj.as_mut().end_reset_model();
        qobj.as_mut().set_count(count);
    });
}

/// Background listener task maintaining taskbar state from daemon services.
#[allow(clippy::too_many_lines)]
pub async fn run_taskbar_listener(qt_thread: cxx_qt::CxxQtThread<qobject::WindowListModel>) {
    let session_conn = match crate::shared_session_conn().await {
        Ok(c) => c,
        Err(err) => {
            warn!("Taskbar listener failed to connect to session bus: {err}");
            dispatch_taskbar(&qt_thread, TaskbarCore::new());
            return;
        }
    };

    let debug_hub = crate::debug::get_debug_hub();
    let _ = crate::debug::register_shell_debug_service(&session_conn).await;

    let dbus_proxy = match DBusProxy::new(&session_conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("Taskbar listener failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
        .ok();

    let taskbar_core = Arc::new(Mutex::new(TaskbarCore::new()));
    let local_windows = Arc::new(Mutex::new(Vec::<WindowItem>::new()));

    // Initial sync
    let sync_catalog_and_pins = |core_arc: Arc<Mutex<TaskbarCore>>, conn: zbus::Connection| async move {
        let mut candidates = Vec::new();
        let mut names = HashMap::new();

        if let Ok(apps_proxy) = Apps1Proxy::builder(&conn).build().await {
            if let Ok(mapping) = apps_proxy.get_app_mapping().await {
                for m in mapping {
                    let wm_class = if m.startup_wm_class.is_empty() {
                        None
                    } else {
                        Some(m.startup_wm_class)
                    };
                    candidates.push(AppCandidate::new(m.id, wm_class, m.icon_name));
                }
            }
            if let Ok(apps) = apps_proxy.get_apps().await {
                for a in apps {
                    names.insert(a.id, a.name);
                }
            }
        }

        let pins = if let Ok(pins_proxy) = Pins1Proxy::builder(&conn).build().await {
            pins_proxy.get().await.unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut lock = core_arc.lock().await;
        lock.set_candidates(candidates, names);
        lock.set_pinned_ids(pins);
    };

    sync_catalog_and_pins(taskbar_core.clone(), session_conn.clone()).await;

    #[cfg(feature = "test-hooks")]
    let skip_initial_list = std::env::var("KYNEX_TEST_MUTANT_DISABLE_LIST").is_ok();
    #[cfg(not(feature = "test-hooks"))]
    let skip_initial_list = false;

    // Initial sync of windows from daemon if available with bounded retry backoff
    let mut initial_windows_synced = false;
    let max_retries = 10;
    let mut retry_delay = std::time::Duration::from_millis(50);
    if !skip_initial_list {
        for attempt in 1..=max_retries {
            if let Ok(proxy) = Windows1Proxy::builder(&session_conn).build().await {
                if let Ok(list) = proxy.list().await {
                    info!(
                        "windows: initial List() returned {} (attempt {attempt})",
                        list.len()
                    );
                    *local_windows.lock().await = list.clone();
                    let mut lock = taskbar_core.lock().await;
                    lock.set_windows(list.clone());
                    debug_hub.update_taskbar(lock.clone(), list).await;
                    debug_hub.set_daemon_connected(true);
                    debug_hub.update_model_readiness(|m| m.windows = true).await;
                    debug_hub
                        .set_last_resync_time(chrono::Utc::now().to_rfc3339())
                        .await;
                    dispatch_taskbar(&qt_thread, lock.clone());
                    initial_windows_synced = true;
                    break;
                }
            }
            if attempt < max_retries {
                info!("windows: kynex-daemon Windows1 not yet available on bus (attempt {attempt}/{max_retries}), retrying in {retry_delay:?}...");
                tokio::time::sleep(retry_delay).await;
                retry_delay = (retry_delay * 2).min(std::time::Duration::from_millis(500));
            }
        }
    }

    if !initial_windows_synced {
        info!("windows: initial List() timed out or daemon unavailable after {max_retries} attempts; initializing empty taskbar");
        let lock = taskbar_core.lock().await;
        dispatch_taskbar(&qt_thread, lock.clone());
    }

    #[cfg(feature = "test-hooks")]
    let skip_subscriptions = std::env::var("KYNEX_TEST_MUTANT_DISABLE_SUBSCRIPTION").is_ok();
    #[cfg(not(feature = "test-hooks"))]
    let skip_subscriptions = false;

    // Set up signal streams
    let mut added_stream = if skip_subscriptions {
        None
    } else {
        match Windows1Proxy::builder(&session_conn).build().await {
            Ok(proxy) => proxy.receive_added().await.ok(),
            Err(_) => None,
        }
    };
    let mut changed_stream = if skip_subscriptions {
        None
    } else {
        match Windows1Proxy::builder(&session_conn).build().await {
            Ok(proxy) => proxy.receive_changed().await.ok(),
            Err(_) => None,
        }
    };
    let mut removed_stream = if skip_subscriptions {
        None
    } else {
        match Windows1Proxy::builder(&session_conn).build().await {
            Ok(proxy) => proxy.receive_removed().await.ok(),
            Err(_) => None,
        }
    };
    let mut active_changed_stream = if skip_subscriptions {
        None
    } else {
        match Windows1Proxy::builder(&session_conn).build().await {
            Ok(proxy) => proxy.receive_active_changed().await.ok(),
            Err(_) => None,
        }
    };
    let mut pins_changed_stream = match Pins1Proxy::builder(&session_conn).build().await {
        Ok(proxy) => proxy.receive_changed().await.ok(),
        Err(_) => None,
    };
    let mut apps_changed_stream = match Apps1Proxy::builder(&session_conn).build().await {
        Ok(proxy) => proxy.receive_changed().await.ok(),
        Err(_) => None,
    };

    let mut dirty = false;
    let mut debounce = tokio::time::interval(std::time::Duration::from_millis(25));
    debounce.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = debounce.tick(), if dirty => {
                dirty = false;
                let windows = local_windows.lock().await.clone();
                let mut lock = taskbar_core.lock().await;
                lock.set_windows(windows.clone());
                debug_hub.update_taskbar(lock.clone(), windows).await;
                dispatch_taskbar(&qt_thread, lock.clone());
            }

            Some(_) = async {
                match &mut pins_changed_stream {
                    Some(s) => s.next().await,
                    None => futures_util::future::pending().await,
                }
            } => {
                info!("Pins changed signal received on taskbar");
                if let Ok(proxy) = Pins1Proxy::builder(&session_conn).build().await {
                    if let Ok(pins) = proxy.get().await {
                        let mut lock = taskbar_core.lock().await;
                        lock.set_pinned_ids(pins);
                        let wins = local_windows.lock().await.clone();
                        debug_hub.update_taskbar(lock.clone(), wins).await;
                        dispatch_taskbar(&qt_thread, lock.clone());
                    }
                }
            }

            Some(_) = async {
                match &mut apps_changed_stream {
                    Some(s) => s.next().await,
                    None => futures_util::future::pending().await,
                }
            } => {
                info!("Apps changed signal received on taskbar");
                sync_catalog_and_pins(taskbar_core.clone(), session_conn.clone()).await;
                let lock = taskbar_core.lock().await;
                let wins = local_windows.lock().await.clone();
                debug_hub.update_taskbar(lock.clone(), wins).await;
                dispatch_taskbar(&qt_thread, lock.clone());
            }

            Some(signal) = async {
                if let Some(ref mut s) = added_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = signal.args() {
                    info!("windows: signal Added id={}", args.id);
                    trace!("windows: Added details: id={}, title='{}', app_id='{}', min={}, act={}", args.id, args.title, args.app_id, args.minimized, args.active);
                    let mut lock = local_windows.lock().await;
                    let item = WindowItem {
                        id: args.id.to_string(),
                        title: args.title.to_string(),
                        app_id: args.app_id.to_string(),
                        minimized: args.minimized,
                        active: args.active,
                    };
                    if item.active {
                        for w in lock.iter_mut() {
                            w.active = false;
                        }
                    }
                    if let Some(existing) = lock.iter_mut().find(|w| w.id == item.id) {
                        *existing = item;
                    } else {
                        lock.push(item);
                    }
                    dirty = true;
                }
            }

            Some(signal) = async {
                if let Some(ref mut s) = changed_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = signal.args() {
                    info!("windows: signal Changed id={}, minimized={}, active={}", args.id, args.minimized, args.active);
                    trace!("windows: Changed details: id={}, title='{}', app_id='{}'", args.id, args.title, args.app_id);
                    let mut lock = local_windows.lock().await;
                    let mut need_update = false;
                    for w in lock.iter() {
                        if w.id == args.id {
                            if w.title != args.title
                                || w.app_id != args.app_id
                                || w.minimized != args.minimized
                                || w.active != args.active
                            {
                                need_update = true;
                            }
                            break;
                        }
                    }
                    if need_update {
                        for w in lock.iter_mut() {
                            if w.id == args.id {
                                w.title = args.title.to_string();
                                w.app_id = args.app_id.to_string();
                                w.minimized = args.minimized;
                                w.active = args.active;
                            } else if args.active {
                                w.active = false;
                            }
                        }
                        dirty = true;
                    }
                }
            }

            Some(signal) = async {
                if let Some(ref mut s) = removed_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = signal.args() {
                    info!("windows: signal Removed id={}", args.id);
                    let mut lock = local_windows.lock().await;
                    lock.retain(|w| w.id != args.id);
                    dirty = true;
                }
            }

            Some(signal) = async {
                if let Some(ref mut s) = active_changed_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = signal.args() {
                    info!("windows: signal ActiveChanged id={}", args.id);
                    let mut lock = local_windows.lock().await;
                    let mut changed = false;
                    for w in lock.iter_mut() {
                        let should_be_active = w.id == args.id;
                        if w.active != should_be_active {
                            w.active = should_be_active;
                            changed = true;
                        }
                    }
                    if changed {
                        dirty = true;
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
                    if new_owner.as_ref().is_none_or(|o| o.as_str().is_empty()) {
                        info!("kynex-daemon left the bus; clearing taskbar windows");
                        debug_hub.set_daemon_connected(false);
                        *local_windows.lock().await = Vec::new();
                        let mut lock = taskbar_core.lock().await;
                        lock.set_windows(Vec::new());
                        debug_hub.update_taskbar(lock.clone(), Vec::new()).await;
                        dispatch_taskbar(&qt_thread, lock.clone());
                    } else {
                        info!("kynex-daemon appeared on the bus (new_owner={:?}); resyncing taskbar", new_owner.as_ref().map(zbus::names::UniqueName::as_str));
                        debug_hub.set_daemon_connected(true);
                        sync_catalog_and_pins(taskbar_core.clone(), session_conn.clone()).await;
                        if let Ok(proxy) = Windows1Proxy::builder(&session_conn).build().await {
                            if let Ok(list) = proxy.list().await {
                                info!("windows: initial List() on daemon connect returned {}", list.len());
                                *local_windows.lock().await = list.clone();
                                let mut lock = taskbar_core.lock().await;
                                lock.set_windows(list.clone());
                                debug_hub.update_taskbar(lock.clone(), list).await;
                                debug_hub.set_last_resync_time(chrono::Utc::now().to_rfc3339()).await;
                                dispatch_taskbar(&qt_thread, lock.clone());
                            }
                            added_stream = proxy.receive_added().await.ok();
                            changed_stream = proxy.receive_changed().await.ok();
                            removed_stream = proxy.receive_removed().await.ok();
                            active_changed_stream = proxy.receive_active_changed().await.ok();
                        }
                        pins_changed_stream = match Pins1Proxy::builder(&session_conn).build().await {
                            Ok(p) => p.receive_changed().await.ok(),
                            Err(_) => None,
                        };
                        apps_changed_stream = match Apps1Proxy::builder(&session_conn).build().await {
                            Ok(p) => p.receive_changed().await.ok(),
                            Err(_) => None,
                        };
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kynex_shell_core::taskbar::TaskbarEntry;

    #[test]
    fn test_role_constants_and_names() {
        let roles = create_window_role_names();

        assert_eq!(
            roles.get(&ROLE_ID).map(|b| b.to_string()),
            Some("id".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_TITLE).map(|b| b.to_string()),
            Some("title".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_APP_ID).map(|b| b.to_string()),
            Some("appId".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_MINIMIZED).map(|b| b.to_string()),
            Some("minimized".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_ACTIVE).map(|b| b.to_string()),
            Some("active".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_ICON_NAME).map(|b| b.to_string()),
            Some("iconName".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_LETTER_BADGE).map(|b| b.to_string()),
            Some("letterBadge".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_IS_PINNED).map(|b| b.to_string()),
            Some("isPinned".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_IS_RUNNING).map(|b| b.to_string()),
            Some("isRunning".to_string())
        );
        assert_eq!(
            roles.get(&ROLE_WINDOW_COUNT).map(|b| b.to_string()),
            Some("windowCount".to_string())
        );
    }

    #[test]
    fn test_window_list_model_data_and_minimized_role() {
        let entries = vec![
            TaskbarEntry {
                app_id: "org.gnome.Calculator.desktop".to_string(),
                name: "Calculator".to_string(),
                icon_name: "org.gnome.Calculator".to_string(),
                letter_badge: "C".to_string(),
                is_pinned: false,
                is_running: true,
                is_active: false,
                is_minimized: true,
                window_count: 1,
                window_ids: vec!["win-1".to_string()],
            },
            TaskbarEntry {
                app_id: "firefox.desktop".to_string(),
                name: "Firefox".to_string(),
                icon_name: "firefox".to_string(),
                letter_badge: "F".to_string(),
                is_pinned: true,
                is_running: true,
                is_active: false,
                is_minimized: false, // in background, but NOT minimized!
                window_count: 1,
                window_ids: vec!["win-2".to_string()],
            },
        ];
        let model = WindowListModelRust {
            entries,
            count: 2,
            ..Default::default()
        };

        // Entry 0: minimized = true
        let v_min_0 = model.data(0, ROLE_MINIMIZED);
        assert_eq!(v_min_0.value::<bool>(), Some(true));
        let v_title_0 = model.data(0, ROLE_TITLE);
        assert_eq!(
            v_title_0.value::<QString>().map(|s| s.to_string()),
            Some("Calculator".to_string())
        );

        // Entry 1: background, minimized = false
        let v_min_1 = model.data(1, ROLE_MINIMIZED);
        assert_eq!(v_min_1.value::<bool>(), Some(false));
        let v_title_1 = model.data(1, ROLE_TITLE);
        assert_eq!(
            v_title_1.value::<QString>().map(|s| s.to_string()),
            Some("Firefox".to_string())
        );

        // Out-of-bounds row
        let v_oob = model.data(99, ROLE_MINIMIZED);
        assert_eq!(v_oob.value::<bool>(), None);
    }
}
