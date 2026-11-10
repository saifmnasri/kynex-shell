//! Launcher model exposing application search and launch state to QML via `cxx-qt`.
//!
//! Subclasses `QAbstractListModel` to provide an asynchronous, reactive search model
//! for the launcher overlay. Automatically reconnects to `org.kynexos.Daemon` when
//! the daemon restarts.

use core::pin::Pin;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use futures_util::StreamExt;
use kynex_ipc::apps::Apps1Proxy;
use kynex_ipc::shell::Shell1Proxy;
use kynex_ipc::DAEMON_DESTINATION;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};
use zbus::fdo::DBusProxy;

use crate::launcher_logic::{LauncherCore, MAX_LAUNCHER_RESULTS};

/// Role ID for desktop entry ID.
pub const ROLE_ID: i32 = 257;
/// Role ID for application display name.
pub const ROLE_NAME: i32 = 258;
/// Role ID for generic application name.
pub const ROLE_GENERIC_NAME: i32 = 259;
/// Role ID for description/comment.
pub const ROLE_COMMENT: i32 = 260;
/// Role ID for single-character placeholder badge.
pub const ROLE_LETTER_BADGE: i32 = 261;
/// Role ID for icon name or path.
pub const ROLE_ICON_NAME: i32 = 262;
/// Role ID for pinned status.
pub const ROLE_IS_PINNED: i32 = 263;

/// CXX-Qt bridge module for `LauncherModel`.
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
        /// Launcher model for search presentation and app launching.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        #[qproperty(i32, selected_index)]
        #[qproperty(bool, visible)]
        #[qproperty(QString, query)]
        type LauncherModel = super::LauncherModelRust;

        /// Initializes background D-Bus listener and search worker.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Returns row count for the model.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &LauncherModel, parent: &QModelIndex) -> i32;

        /// Returns role data for a given index and role.
        #[qinvokable]
        #[cxx_override]
        fn data(self: &LauncherModel, index: &QModelIndex, role: i32) -> QVariant;

        /// Returns role names mapping for QML property bindings.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &LauncherModel) -> QHash_i32_QByteArray;

        /// Submits a search query with latest-wins sequence tracking.
        #[qinvokable]
        #[cxx_name = "setSearchQuery"]
        fn set_search_query(self: Pin<&mut Self>, query: &QString);

        /// Moves selection down with wrap-around.
        #[qinvokable]
        #[cxx_name = "selectNext"]
        fn select_next(self: Pin<&mut Self>);

        /// Moves selection up with wrap-around.
        #[qinvokable]
        #[cxx_name = "selectPrev"]
        fn select_prev(self: Pin<&mut Self>);

        /// Launches the application at row `index`.
        #[qinvokable]
        fn launch(self: Pin<&mut Self>, index: i32);

        /// Launches the currently selected application.
        #[qinvokable]
        #[cxx_name = "launchSelected"]
        fn launch_selected(self: Pin<&mut Self>);

        /// Closes the launcher overlay and clears query.
        #[qinvokable]
        fn close(self: Pin<&mut Self>);

        /// Records the first frame rendered after the launcher surface was mapped.
        #[qinvokable]
        #[cxx_name = "recordFirstFrame"]
        fn record_first_frame(self: Pin<&mut Self>);

        /// Toggles overlay visibility.
        #[qinvokable]
        #[cxx_name = "toggleVisible"]
        fn toggle_visible(self: Pin<&mut Self>);

        /// Toggles pinned status of currently selected application.
        #[qinvokable]
        #[cxx_name = "togglePinSelected"]
        fn toggle_pin_selected(self: Pin<&mut Self>);
    }

    unsafe extern "RustQt" {
        /// Inherited beginResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut LauncherModel>);

        /// Inherited endResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut LauncherModel>);
    }

    impl cxx_qt::Threading for LauncherModel {}
}

/// Backing data for `LauncherModel`.
#[derive(Default)]
pub struct LauncherModelRust {
    core: LauncherCore,
    count: i32,
    selected_index: i32,
    visible: bool,
    query: QString,
    search_tx: Option<mpsc::UnboundedSender<(u64, String)>>,
    open_start: Option<std::time::Instant>,
    last_keystroke_start: Option<std::time::Instant>,
}

impl qobject::LauncherModel {
    /// Initializes background D-Bus listener and worker.
    pub fn init(mut self: Pin<&mut Self>) {
        let (tx, rx) = mpsc::unbounded_channel::<(u64, String)>();
        self.as_mut().rust_mut().search_tx = Some(tx.clone());

        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_launcher_service(qt_thread, rx).await;
        });

        // Trigger initial search for full application list
        let (seq, empty) = self.as_mut().rust_mut().core.close();
        let _ = tx.send((seq, empty));
    }

    /// Returns row count for the model.
    #[must_use]
    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        i32::try_from(self.rust().core.count()).unwrap_or(0)
    }

    /// Returns role data for a given index and role.
    #[must_use]
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let row = index.row();
        let Ok(row_idx) = usize::try_from(row) else {
            return QVariant::default();
        };

        let items = self.rust().core.items();
        if row_idx >= items.len() {
            return QVariant::default();
        }

        let item = &items[row_idx];
        match role {
            ROLE_ID => QVariant::from(&QString::from(&item.id)),
            ROLE_NAME => QVariant::from(&QString::from(&item.name)),
            ROLE_GENERIC_NAME => QVariant::from(&QString::from(&item.generic_name)),
            ROLE_COMMENT => QVariant::from(&QString::from(&item.comment)),
            ROLE_LETTER_BADGE => QVariant::from(&QString::from(&item.letter_badge)),
            ROLE_ICON_NAME => QVariant::from(&QString::from(&item.icon_name)),
            ROLE_IS_PINNED => QVariant::from(&item.is_pinned),
            _ => QVariant::default(),
        }
    }

    /// Returns role names mapping for QML.
    #[must_use]
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_ID, QByteArray::from("id"));
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_GENERIC_NAME, QByteArray::from("genericName"));
        roles.insert(ROLE_COMMENT, QByteArray::from("comment"));
        roles.insert(ROLE_LETTER_BADGE, QByteArray::from("letterBadge"));
        roles.insert(ROLE_ICON_NAME, QByteArray::from("iconName"));
        roles.insert(ROLE_IS_PINNED, QByteArray::from("isPinned"));
        roles
    }

    /// Submits a search query with latest-wins sequence tracking.
    pub fn set_search_query(mut self: Pin<&mut Self>, query: &QString) {
        let raw = query.to_string();
        let (seq, clean) = self.as_mut().rust_mut().core.prepare_search_query(&raw);
        self.as_mut().rust_mut().last_keystroke_start = Some(std::time::Instant::now());
        self.as_mut().set_query(query.clone());

        let tx_opt = self.rust().search_tx.clone();
        if let Some(tx) = tx_opt {
            let _ = tx.send((seq, clean));
        }
    }

    /// Moves selection down with wrap-around.
    pub fn select_next(mut self: Pin<&mut Self>) {
        let next = self.as_mut().rust_mut().core.select_next();
        self.as_mut().set_selected_index(next);
    }

    /// Moves selection up with wrap-around.
    pub fn select_prev(mut self: Pin<&mut Self>) {
        let prev = self.as_mut().rust_mut().core.select_prev();
        self.as_mut().set_selected_index(prev);
    }

    /// Launches the application at row `index`.
    pub fn launch(self: Pin<&mut Self>, index: i32) {
        if let Some(app_id) = self.rust().core.launch(index) {
            tokio::spawn(async move {
                match zbus::connection::Builder::session().build().await {
                    Ok(conn) => match Apps1Proxy::new(&conn).await {
                        Ok(proxy) => {
                            if let Err(err) = proxy.launch(&app_id).await {
                                error!("Failed to launch {app_id}: {err}");
                            }
                        }
                        Err(err) => error!("Failed to create Apps1Proxy: {err}"),
                    },
                    Err(err) => error!("Failed to connect to session bus: {err}"),
                }
            });
        }

        self.close();
    }

    /// Launches the currently selected application.
    pub fn launch_selected(self: Pin<&mut Self>) {
        let selected = self.rust().core.selected_index();
        self.launch(selected);
    }

    /// Closes the launcher overlay and clears query.
    pub fn close(mut self: Pin<&mut Self>) {
        let (seq, empty) = self.as_mut().rust_mut().core.close();
        self.as_mut().rust_mut().open_start = None;
        self.as_mut().set_visible(false);
        self.as_mut().set_query(QString::default());
        self.as_mut().set_selected_index(-1);
        let tx_opt = self.rust().search_tx.clone();
        if let Some(tx) = tx_opt {
            let _ = tx.send((seq, empty));
        }
    }

    /// Records the first frame rendered after the launcher surface was mapped.
    pub fn record_first_frame(mut self: Pin<&mut Self>) {
        if let Some(start) = self.as_mut().rust_mut().open_start.take() {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            tracing::info!(target: "kynex_perf", latency_ms = elapsed_ms, "Launcher shortcut-to-first-frame mapped");
        }
    }

    /// Toggles overlay visibility.
    pub fn toggle_visible(mut self: Pin<&mut Self>) {
        let current = self.rust().visible;
        let new_vis = !current;
        if new_vis {
            self.as_mut().rust_mut().open_start = Some(std::time::Instant::now());
        } else {
            self.as_mut().rust_mut().open_start = None;
        }
        self.as_mut().set_visible(new_vis);
        if new_vis {
            self.as_mut().set_query(QString::default());
            let (seq, empty) = self.as_mut().rust_mut().core.close();
            let tx_opt = self.rust().search_tx.clone();
            if let Some(tx) = tx_opt {
                let _ = tx.send((seq, empty));
            }
        }
    }

    /// Toggles pinned status of the selected application.
    pub fn toggle_pin_selected(mut self: Pin<&mut Self>) {
        let toggled = self.as_mut().rust_mut().core.toggle_pin_selected();
        if let Some((app_id, will_pin)) = toggled {
            self.as_mut().begin_reset_model();
            self.as_mut().end_reset_model();

            tokio::spawn(async move {
                if let Ok(conn) = zbus::connection::Builder::session().build().await {
                    if let Ok(proxy) = kynex_ipc::pins::Pins1Proxy::new(&conn).await {
                        if will_pin {
                            let _ = proxy.pin(&app_id).await;
                        } else {
                            let _ = proxy.unpin(&app_id).await;
                        }
                    }
                }
            });
        }
    }
}

/// Dispatches search results to the Qt main thread subject to sequence verification.
fn dispatch_search_results(
    qt_thread: &cxx_qt::CxxQtThread<qobject::LauncherModel>,
    seq: u64,
    items: Vec<kynex_ipc::apps::AppItem>,
) {
    let _ = qt_thread.queue(move |mut qobj| {
        qobj.as_mut().begin_reset_model();
        let applied = qobj.as_mut().rust_mut().core.apply_results(seq, &items);
        let count = i32::try_from(qobj.rust().core.count()).unwrap_or(0);
        let selected = qobj.rust().core.selected_index();
        qobj.as_mut().end_reset_model();

        if !applied {
            debug!("Discarding stale search result with seq {seq}");
            return;
        }

        qobj.as_mut().set_count(count);
        qobj.as_mut().set_selected_index(selected);

        if let Some(start) = qobj.as_mut().rust_mut().last_keystroke_start.take() {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            tracing::info!(target: "kynex_perf", latency_ms = elapsed_ms, "Launcher keystroke search results updated");
        }
    });
}

/// Background service managing D-Bus signals and search queries.
#[allow(clippy::too_many_lines)]
async fn run_launcher_service(
    qt_thread: cxx_qt::CxxQtThread<qobject::LauncherModel>,
    mut search_rx: mpsc::UnboundedReceiver<(u64, String)>,
) {
    let session_conn = match zbus::connection::Builder::session().build().await {
        Ok(c) => c,
        Err(err) => {
            warn!("LauncherModel failed to connect to session bus: {err}");
            return;
        }
    };

    let dbus_proxy = match DBusProxy::new(&session_conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("LauncherModel failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
        .ok();

    // Setup initial signal subscriptions
    let mut shell_proxy = Shell1Proxy::builder(&session_conn).build().await.ok();
    let mut toggle_stream = match shell_proxy {
        Some(ref p) => p.receive_launcher_toggle_requested().await.ok(),
        None => None,
    };

    let mut apps_proxy = Apps1Proxy::builder(&session_conn).build().await.ok();
    let mut changed_stream = match apps_proxy {
        Some(ref p) => p.receive_changed().await.ok(),
        None => None,
    };

    let pins_proxy = kynex_ipc::pins::Pins1Proxy::builder(&session_conn)
        .build()
        .await
        .ok();
    if let Some(ref p) = pins_proxy {
        if let Ok(pins) = p.get().await {
            let _ = qt_thread.queue(move |mut qobj| {
                qobj.as_mut().rust_mut().core.set_pinned_ids(pins);
            });
        }
    }
    let mut pins_changed_stream = match pins_proxy {
        Some(ref p) => p.receive_changed().await.ok(),
        None => None,
    };

    let latest_query = Arc::new(Mutex::new(String::new()));
    info!("LauncherModel connected to session bus");

    loop {
        tokio::select! {
            // New search request from UI
            Some((seq, query)) = search_rx.recv() => {
                *latest_query.lock().await = query.clone();
                let proxy_opt = Apps1Proxy::builder(&session_conn).build().await.ok();
                if let Some(proxy) = proxy_opt {
                    let results = proxy.search(&query, MAX_LAUNCHER_RESULTS).await;
                    match results {
                        Ok(raw_items) => {
                            dispatch_search_results(&qt_thread, seq, raw_items);
                        }
                        Err(err) => {
                            warn!("Apps1.Search failed: {err}");
                            dispatch_search_results(&qt_thread, seq, Vec::new());
                        }
                    }
                } else {
                    dispatch_search_results(&qt_thread, seq, Vec::new());
                }
            }

            // Launcher toggle requested via global shortcut / KWin
            Some(_) = async {
                match &mut toggle_stream {
                    Some(s) => s.next().await,
                    None => futures_util::future::pending().await,
                }
            } => {
                let recv_instant = std::time::Instant::now();
                info!("Launcher toggle requested received over D-Bus");
                let _ = qt_thread.queue(move |mut qobj| {
                    let current = qobj.rust().visible;
                    let new_vis = !current;
                    if new_vis {
                        qobj.as_mut().rust_mut().open_start = Some(recv_instant);
                    } else {
                        qobj.as_mut().rust_mut().open_start = None;
                    }
                    qobj.as_mut().set_visible(new_vis);
                    if new_vis {
                        qobj.as_mut().set_query(QString::default());
                        let (seq, empty) = qobj.as_mut().rust_mut().core.close();
                        let tx_opt = qobj.rust().search_tx.clone();
                        if let Some(tx) = tx_opt {
                            let _ = tx.send((seq, empty));
                        }
                    }
                });
            }

            // Application index changed on filesystem
            Some(_) = async {
                match &mut changed_stream {
                    Some(s) => s.next().await,
                    None => futures_util::future::pending().await,
                }
            } => {
                info!("Apps index changed; refreshing launcher search");
                let q = latest_query.lock().await.clone();
                if let Ok(proxy) = Apps1Proxy::builder(&session_conn).build().await {
                    if let Ok(raw_items) = proxy.search(&q, MAX_LAUNCHER_RESULTS).await {
                        let _ = qt_thread.queue(move |mut qobj| {
                            let current_seq = qobj.rust().core.current_seq();
                            qobj.as_mut().begin_reset_model();
                            let applied = qobj.as_mut().rust_mut().core.apply_results(current_seq, &raw_items);
                            let count = i32::try_from(qobj.rust().core.count()).unwrap_or(0);
                            let selected = qobj.rust().core.selected_index();
                            qobj.as_mut().end_reset_model();
                            if applied {
                                qobj.as_mut().set_count(count);
                                qobj.as_mut().set_selected_index(selected);
                            }
                        });
                    }
                }
            }

            // Pinned applications changed
            Some(_) = async {
                match &mut pins_changed_stream {
                    Some(s) => s.next().await,
                    None => futures_util::future::pending().await,
                }
            } => {
                info!("Pins changed; refreshing launcher pinned status");
                if let Ok(proxy) = kynex_ipc::pins::Pins1Proxy::builder(&session_conn).build().await {
                    if let Ok(pins) = proxy.get().await {
                        let _ = qt_thread.queue(move |mut qobj| {
                            qobj.as_mut().begin_reset_model();
                            qobj.as_mut().rust_mut().core.set_pinned_ids(pins);
                            qobj.as_mut().end_reset_model();
                        });
                    }
                }
            }

            // Daemon lifecycle (restarts)
            Some(signal) = async {
                match &mut name_owner_stream {
                    Some(s) => s.next().await,
                    None => futures_util::future::pending().await,
                }
            } => {
                if let Ok(args) = signal.args() {
                    let new_owner = args.new_owner();
                    if new_owner.as_ref().is_none_or(|o| o.as_str().is_empty()) {
                        info!("kynex-daemon left the bus");
                        toggle_stream = None;
                        changed_stream = None;
                    } else {
                        info!("kynex-daemon appeared/restarted on bus; re-subscribing");
                        shell_proxy = Shell1Proxy::builder(&session_conn).build().await.ok();
                        toggle_stream = match shell_proxy {
                            Some(ref p) => p.receive_launcher_toggle_requested().await.ok(),
                            None => None,
                        };

                        apps_proxy = Apps1Proxy::builder(&session_conn).build().await.ok();
                        changed_stream = match apps_proxy {
                            Some(ref p) => p.receive_changed().await.ok(),
                            None => None,
                        };

                        // Re-query latest search
                        let q = latest_query.lock().await.clone();
                        if let Some(ref proxy) = apps_proxy {
                            if let Ok(raw_items) = proxy.search(&q, MAX_LAUNCHER_RESULTS).await {
                                let _ = qt_thread.queue(move |mut qobj| {
                                    let current_seq = qobj.rust().core.current_seq();
                                    qobj.as_mut().begin_reset_model();
                                    let applied = qobj.as_mut().rust_mut().core.apply_results(current_seq, &raw_items);
                                    let count = i32::try_from(qobj.rust().core.count()).unwrap_or(0);
                                    let selected = qobj.rust().core.selected_index();
                                    qobj.as_mut().end_reset_model();
                                    if applied {
                                        qobj.as_mut().set_count(count);
                                        qobj.as_mut().set_selected_index(selected);
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}
