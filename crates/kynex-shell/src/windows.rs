//! Window list model exposing window tracking state to QML via `cxx-qt`.
//!
//! Subclasses `QAbstractListModel` to provide a reactive, zero-polling data model
//! for shell taskbars and window switchers. Automatically reconnects to
//! `org.kynexos.Daemon.Windows1` when the daemon restarts.

use core::pin::Pin;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use futures_util::StreamExt;
use kynex_ipc::windows::{WindowItem, Windows1Proxy};
use kynex_ipc::DAEMON_DESTINATION;
use tokio::sync::Mutex;
use tracing::{info, warn};
use zbus::fdo::DBusProxy;
use zbus::names::WellKnownName;

/// Role IDs for `WindowListModel`.
pub const ROLE_ID: i32 = 257;
/// Title role ID.
pub const ROLE_TITLE: i32 = 258;
/// Application ID role ID.
pub const ROLE_APP_ID: i32 = 259;
/// Minimized state role ID.
pub const ROLE_MINIMIZED: i32 = 260;
/// Active focus state role ID.
pub const ROLE_ACTIVE: i32 = 261;

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
        /// Window list model for QML presentation.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        type WindowListModel = super::WindowListModelRust;

        /// Initializes the background D-Bus listener.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Returns the number of rows (windows) in the model.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &WindowListModel, parent: &QModelIndex) -> i32;

        /// Returns the data for a given role and model index.
        #[qinvokable]
        #[cxx_override]
        fn data(self: &WindowListModel, index: &QModelIndex, role: i32) -> QVariant;

        /// Returns the role names map for QML property bindings.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &WindowListModel) -> QHash_i32_QByteArray;

        /// Activates (focuses) the window matching `id`.
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
    windows: Vec<WindowItem>,
    count: i32,
}

impl qobject::WindowListModel {
    /// Initializes background listener connecting to `kynex-daemon`.
    pub fn init(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_windows_listener(qt_thread).await;
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

        let windows = &self.rust().windows;
        if row_idx >= windows.len() {
            return QVariant::default();
        }

        let item = &windows[row_idx];
        match role {
            ROLE_ID => QVariant::from(&QString::from(&item.id)),
            ROLE_TITLE => QVariant::from(&QString::from(&item.title)),
            ROLE_APP_ID => QVariant::from(&QString::from(&item.app_id)),
            ROLE_MINIMIZED => QVariant::from(&item.minimized),
            ROLE_ACTIVE => QVariant::from(&item.active),
            _ => QVariant::default(),
        }
    }

    /// Returns role names mapping for QML.
    #[must_use]
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_ID, QByteArray::from("id"));
        roles.insert(ROLE_TITLE, QByteArray::from("title"));
        roles.insert(ROLE_APP_ID, QByteArray::from("appId"));
        roles.insert(ROLE_MINIMIZED, QByteArray::from("minimized"));
        roles.insert(ROLE_ACTIVE, QByteArray::from("active"));
        roles
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

/// Dispatches an updated windows vector to the Qt main thread.
fn dispatch_windows(
    qt_thread: &cxx_qt::CxxQtThread<qobject::WindowListModel>,
    windows: Vec<WindowItem>,
) {
    let count = i32::try_from(windows.len()).unwrap_or(0);
    let _ = qt_thread.queue(move |mut qobj| {
        qobj.as_mut().begin_reset_model();
        qobj.as_mut().rust_mut().windows = windows;
        qobj.as_mut().rust_mut().count = count;
        qobj.as_mut().end_reset_model();
        qobj.as_mut().set_count(count);
    });
}

/// Background listener task maintaining window tracking state from `kynex-daemon`.
#[allow(clippy::too_many_lines)]
pub async fn run_windows_listener(qt_thread: cxx_qt::CxxQtThread<qobject::WindowListModel>) {
    let session_conn = match zbus::connection::Builder::session().build().await {
        Ok(c) => c,
        Err(err) => {
            warn!("WindowListModel failed to connect to session bus: {err}");
            dispatch_windows(&qt_thread, Vec::new());
            return;
        }
    };

    let dbus_proxy = match DBusProxy::new(&session_conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("WindowListModel failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
        .ok();

    let local_windows = Arc::new(Mutex::new(Vec::<WindowItem>::new()));

    // Check initial daemon presence
    let wk_name: Result<WellKnownName<'_>, _> = DAEMON_DESTINATION.try_into();
    let has_daemon = match wk_name {
        Ok(ref name) => dbus_proxy
            .get_name_owner(zbus::names::BusName::WellKnown(name.as_ref()))
            .await
            .is_ok(),
        Err(_) => false,
    };

    if has_daemon {
        if let Ok(proxy) = Windows1Proxy::builder(&session_conn).build().await {
            if let Ok(list) = proxy.list().await {
                info!(
                    "WindowListModel connected to daemon: {} windows",
                    list.len()
                );
                *local_windows.lock().await = list.clone();
                dispatch_windows(&qt_thread, list);
            }
        }
    } else {
        info!("kynex-daemon not found on bus; setting empty windows");
        dispatch_windows(&qt_thread, Vec::new());
    }

    // Set up signal proxies
    let mut added_stream = match Windows1Proxy::builder(&session_conn).build().await {
        Ok(proxy) => proxy.receive_added().await.ok(),
        Err(_) => None,
    };
    let mut changed_stream = match Windows1Proxy::builder(&session_conn).build().await {
        Ok(proxy) => proxy.receive_changed().await.ok(),
        Err(_) => None,
    };
    let mut removed_stream = match Windows1Proxy::builder(&session_conn).build().await {
        Ok(proxy) => proxy.receive_removed().await.ok(),
        Err(_) => None,
    };
    let mut active_changed_stream = match Windows1Proxy::builder(&session_conn).build().await {
        Ok(proxy) => proxy.receive_active_changed().await.ok(),
        Err(_) => None,
    };

    let mut dirty = false;
    let mut debounce = tokio::time::interval(std::time::Duration::from_millis(25));
    debounce.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = debounce.tick(), if dirty => {
                dirty = false;
                let current = local_windows.lock().await.clone();
                dispatch_windows(&qt_thread, current);
            }

            Some(signal) = async {
                if let Some(ref mut s) = added_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = signal.args() {
                    let mut lock = local_windows.lock().await;
                    let item = WindowItem {
                        id: args.id.to_string(),
                        title: args.title.to_string(),
                        app_id: args.app_id.to_string(),
                        minimized: args.minimized,
                        active: args.active,
                    };
                    info!("WindowListModel: window added '{}' (active={}, minimized={})", item.title, item.active, item.minimized);
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
                    let mut lock = local_windows.lock().await;
                    info!("WindowListModel: window changed '{}' (active={}, minimized={})", args.title, args.active, args.minimized);
                    if args.active {
                        for w in lock.iter_mut() {
                            w.active = false;
                        }
                    }
                    if let Some(existing) = lock.iter_mut().find(|w| w.id == args.id) {
                        existing.title = args.title.to_string();
                        existing.app_id = args.app_id.to_string();
                        existing.minimized = args.minimized;
                        existing.active = args.active;
                    }
                    dirty = true;
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
                    let mut lock = local_windows.lock().await;
                    info!("WindowListModel: window removed '{}'", args.id);
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
                    let mut lock = local_windows.lock().await;
                    info!("WindowListModel: active window changed to '{}'", args.id);
                    for w in lock.iter_mut() {
                        w.active = w.id == args.id;
                    }
                    dirty = true;
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
                        info!("kynex-daemon left the bus; clearing WindowListModel");
                        *local_windows.lock().await = Vec::new();
                        dispatch_windows(&qt_thread, Vec::new());
                    } else {
                        info!("kynex-daemon appeared on the bus; resyncing WindowListModel");
                        if let Ok(proxy) = Windows1Proxy::builder(&session_conn).build().await {
                            if let Ok(list) = proxy.list().await {
                                *local_windows.lock().await = list.clone();
                                dispatch_windows(&qt_thread, list);
                            }
                            added_stream = proxy.receive_added().await.ok();
                            changed_stream = proxy.receive_changed().await.ok();
                            removed_stream = proxy.receive_removed().await.ok();
                            active_changed_stream = proxy.receive_active_changed().await.ok();
                        }
                    }
                }
            }
        }
    }
}
