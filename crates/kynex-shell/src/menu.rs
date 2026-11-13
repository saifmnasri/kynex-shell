//! Shell UI model and CXX-Qt bridge for system tray dbusmenu presentation.
//!
//! Enforces:
//! - Non-blocking asynchronous interaction: fetching and events never block the Qt GUI thread.
//! - Pure state delegation to [`MenuCoreState`] in `kynex-shell-core`.
//! - Layout revision tracking on user interactions to prevent stale event dispatch (AUD-P-02).
//! - Pure coordinate projection and edge flipping across all four panel anchors (P2.2).
//! - Performance measurement: tracks cross-process monotonic timestamp to first frame swapped.
//! - Automatic reconnection and invalidation when the daemon restarts.
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
    clippy::bool_assert_comparison,
    clippy::float_cmp,
    clippy::cast_possible_wrap,
    clippy::unwrap_used
)]

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use futures_util::StreamExt;
use kynex_ipc::tray::{MenuItemDto, Tray1Proxy, TRAY_INTERFACE, TRAY_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use kynex_shell_core::coordinates::{OutputGeometry, PanelSize, Point, SurfaceAnchors};
pub use kynex_shell_core::menu::{
    compute_menu_placement, compute_menu_placement_global, compute_menu_popup_geometry,
    handle_menu_name_owner_changed, menu_item_role_value, menu_role_names,
    should_update_on_menu_changed, MenuCoreState, MenuLatencyTracker, MenuRoleValue,
    ROLE_MENU_DISPOSITION, ROLE_MENU_ENABLED, ROLE_MENU_HAS_SUBMENU, ROLE_MENU_ICON_NAME,
    ROLE_MENU_ID, ROLE_MENU_KIND, ROLE_MENU_LABEL, ROLE_MENU_PARENT_ID, ROLE_MENU_SHORTCUT,
    ROLE_MENU_TOGGLE, ROLE_MENU_TOGGLE_STATE, ROLE_MENU_VISIBLE,
};
use tracing::warn;
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::Connection;

/// Constructs the role names hash map mapping role IDs to QML property strings.
#[must_use]
pub fn create_menu_role_names() -> QHash<QHashPair_i32_QByteArray> {
    let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
    for &(role_id, role_name) in menu_role_names() {
        roles.insert(role_id, QByteArray::from(role_name));
    }
    roles
}

/// Converts a menu item and role ID into a `QVariant`.
#[must_use]
pub fn menu_item_role_variant(item: &MenuItemDto, role: i32) -> QVariant {
    match menu_item_role_value(item, role) {
        Some(MenuRoleValue::Text(s)) => QVariant::from(&QString::from(&s)),
        Some(MenuRoleValue::Bool(b)) => QVariant::from(&b),
        Some(MenuRoleValue::Int(i)) => QVariant::from(&i),
        None => QVariant::default(),
    }
}

/// CXX-Qt bridge module for `TrayMenuModel`.
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
        /// System tray context menu list model.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        #[qproperty(bool, visible)]
        #[qproperty(f64, menu_x)]
        #[qproperty(f64, menu_y)]
        #[qproperty(i32, selected_index)]
        #[qproperty(i32, active_parent_id)]
        #[qproperty(QString, item_id)]
        type TrayMenuModel = super::TrayMenuModelRust;

        /// Initializes background D-Bus listeners and monitors daemon lifecycle.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Returns the number of items in the currently visible menu level.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TrayMenuModel, parent: &QModelIndex) -> i32;

        /// Returns role data for a given model index and role.
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TrayMenuModel, index: &QModelIndex, role: i32) -> QVariant;

        /// Returns role names mapping for QML property bindings.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TrayMenuModel) -> QHash_i32_QByteArray;

        /// Opens the menu for a tray item at anchor location with panel geometry and edge anchors.
        #[qinvokable]
        fn open(
            self: Pin<&mut Self>,
            item_id: &QString,
            anchor_x: f64,
            anchor_y: f64,
            anchor_width: f64,
            anchor_height: f64,
            menu_width: f64,
            menu_height: f64,
            panel_anchors: i32,
            panel_width: f64,
            panel_height: f64,
            screen_x: f64,
            screen_y: f64,
            screen_width: f64,
            screen_height: f64,
            device_pixel_ratio: f64,
        );

        /// Closes the menu popup.
        #[qinvokable]
        fn close(self: Pin<&mut Self>);

        /// Activates an item by menu item id (sends MenuEvent with current layout revision then closes).
        #[qinvokable]
        #[cxx_name = "activateItem"]
        fn activate_item(self: Pin<&mut Self>, menu_item_id: i32);

        /// Navigates selection to the next enabled, non-separator item.
        #[qinvokable]
        #[cxx_name = "selectNext"]
        fn select_next(self: Pin<&mut Self>);

        /// Navigates selection to the previous enabled, non-separator item.
        #[qinvokable]
        #[cxx_name = "selectPrev"]
        fn select_prev(self: Pin<&mut Self>);

        /// Navigates selection to the first enabled item.
        #[qinvokable]
        #[cxx_name = "selectFirst"]
        fn select_first(self: Pin<&mut Self>);

        /// Navigates selection to the last enabled item.
        #[qinvokable]
        #[cxx_name = "selectLast"]
        fn select_last(self: Pin<&mut Self>);

        /// Drills down into the submenu of the currently selected item.
        #[qinvokable]
        #[cxx_name = "openSubmenu"]
        fn open_submenu(self: Pin<&mut Self>);

        /// Pops back up to the parent menu level, or closes menu if at root.
        #[qinvokable]
        #[cxx_name = "closeSubmenu"]
        fn close_submenu(self: Pin<&mut Self>);

        /// Activates the currently selected item.
        #[qinvokable]
        #[cxx_name = "activateSelected"]
        fn activate_selected(self: Pin<&mut Self>);

        /// Records the first frame rendered after menu open request.
        #[qinvokable]
        #[cxx_name = "recordFirstFrame"]
        fn record_first_frame(self: Pin<&mut Self>);
    }

    unsafe extern "RustQt" {
        /// Inherited beginResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut TrayMenuModel>);

        /// Inherited endResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut TrayMenuModel>);
    }

    impl cxx_qt::Threading for TrayMenuModel {}
}

/// Backing Rust data for `TrayMenuModel`.
#[derive(Default)]
pub struct TrayMenuModelRust {
    pub(crate) state: MenuCoreState,
    pub(crate) count: i32,
    pub(crate) visible: bool,
    pub(crate) menu_x: f64,
    pub(crate) menu_y: f64,
    pub(crate) selected_index: i32,
    pub(crate) active_parent_id: i32,
    pub(crate) item_id: QString,
    pub(crate) revision: u32,
    pub(crate) latency_tracker: MenuLatencyTracker,
}

impl TrayMenuModelRust {
    /// Returns the number of items in the current active menu level.
    #[must_use]
    pub fn row_count(&self) -> i32 {
        self.count
    }

    /// Sets items and layout revision into state.
    pub fn set_items(&mut self, items: Vec<MenuItemDto>, revision: u32) {
        self.revision = revision;
        self.state.set_items_with_revision(items, revision);
        self.count = i32::try_from(self.state.active_items().len()).unwrap_or(0);
        self.selected_index = self
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.active_parent_id = self.state.current_parent_id();
    }

    /// Clears the model state.
    pub fn clear(&mut self) {
        self.state.clear();
        self.count = 0;
        self.visible = false;
        self.selected_index = -1;
        self.active_parent_id = 0;
        self.revision = 0;
    }

    /// Retrieves role data for a given row and role.
    #[must_use]
    pub fn data(&self, row: i32, role: i32) -> QVariant {
        let Ok(row_idx) = usize::try_from(row) else {
            return QVariant::default();
        };

        let active = self.state.active_items();
        if let Some(item) = active.get(row_idx) {
            menu_item_role_variant(item, role)
        } else {
            QVariant::default()
        }
    }

    /// Moves selection down to the next selectable item.
    pub fn select_next(&mut self) -> i32 {
        self.state.select_next();
        self.selected_index = self
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.selected_index
    }

    /// Moves selection up to the previous selectable item.
    pub fn select_prev(&mut self) -> i32 {
        self.state.select_prev();
        self.selected_index = self
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.selected_index
    }

    /// Selects the first selectable item.
    pub fn select_first(&mut self) -> i32 {
        self.state.select_first();
        self.selected_index = self
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.selected_index
    }

    /// Selects the last selectable item.
    pub fn select_last(&mut self) -> i32 {
        self.state.select_last();
        self.selected_index = self
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.selected_index
    }

    /// Enters submenu of the currently selected item.
    pub fn open_submenu(&mut self) -> Option<i32> {
        let sub_id = self.state.enter_submenu()?;
        self.count = i32::try_from(self.state.active_items().len()).unwrap_or(0);
        self.selected_index = self
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.active_parent_id = sub_id;
        Some(sub_id)
    }

    /// Leaves current submenu, returning true if popped back up.
    pub fn close_submenu(&mut self) -> bool {
        let was_sub = self.state.leave_submenu();
        if was_sub {
            self.count = i32::try_from(self.state.active_items().len()).unwrap_or(0);
            self.selected_index = self
                .state
                .selected_index()
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1);
            self.active_parent_id = self.state.current_parent_id();
        }
        was_sub
    }

    /// Determines the action for activating the currently selected item: returns `(id, has_submenu)`.
    #[must_use]
    pub fn activate_selected(&self) -> Option<(i32, bool)> {
        let item = self.state.selected_item()?;
        if item.has_submenu {
            Some((item.id, true))
        } else if item.kind == 0 && item.enabled {
            Some((item.id, false))
        } else {
            None
        }
    }

    /// Records the first frame rendered after opening the menu.
    pub fn record_first_frame(&mut self) -> Option<std::time::Duration> {
        self.latency_tracker.record_first_frame()
    }
}

/// Sends a `MenuEvent` call over D-Bus with layout revision.
pub async fn send_menu_event_call(
    item_id: &str,
    menu_item_id: i32,
    event: &str,
    revision: u32,
) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let proxy = Tray1Proxy::builder(&conn)
        .destination(DAEMON_DESTINATION)
        .path(TRAY_PATH)
        .build()
        .await?;
    proxy
        .menu_event(item_id, menu_item_id, event, revision)
        .await
}

/// Sends a `MenuAboutToShow` call over D-Bus.
pub async fn send_menu_about_to_show_call(item_id: &str, menu_item_id: i32) -> zbus::Result<bool> {
    let conn = Connection::session().await?;
    let proxy = Tray1Proxy::builder(&conn)
        .destination(DAEMON_DESTINATION)
        .path(TRAY_PATH)
        .build()
        .await?;
    proxy.menu_about_to_show(item_id, menu_item_id).await
}

/// Fetches layout and revision for an item over D-Bus.
pub async fn fetch_menu_layout(item_id: &str) -> zbus::Result<(u32, Vec<MenuItemDto>)> {
    let conn = Connection::session().await?;
    let proxy = Tray1Proxy::builder(&conn)
        .destination(DAEMON_DESTINATION)
        .path(TRAY_PATH)
        .build()
        .await?;
    proxy.get_menu(item_id).await
}

/// Parses a D-Bus signal message for `MenuChanged`. Returns `Some((item_id, revision))` if matched.
#[must_use]
pub fn parse_menu_changed_signal(msg: &zbus::Message) -> Option<(String, u32)> {
    let header = msg.header();
    if msg.message_type() == Type::Signal
        && header
            .interface()
            .is_some_and(|i| i.as_str() == TRAY_INTERFACE)
        && header.member().is_some_and(|m| m.as_str() == "MenuChanged")
    {
        msg.body().deserialize::<(String, u32)>().ok()
    } else {
        None
    }
}

impl qobject::TrayMenuModel {
    /// Initializes background D-Bus listeners and monitors daemon lifecycle.
    pub fn init(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_menu_listener(qt_thread).await;
        });
    }

    /// Returns row count for QAbstractListModel.
    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().row_count()
    }

    /// Retrieves role data for QAbstractListModel.
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        self.rust().data(index.row(), role)
    }

    /// Returns role names mapping for QML.
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        create_menu_role_names()
    }

    /// Opens the menu for a tray item at anchor location with full panel anchors and output geometry.
    pub fn open(
        mut self: Pin<&mut Self>,
        item_id: &QString,
        anchor_x: f64,
        anchor_y: f64,
        anchor_width: f64,
        anchor_height: f64,
        menu_width: f64,
        menu_height: f64,
        panel_anchors: i32,
        panel_width: f64,
        panel_height: f64,
        screen_x: f64,
        screen_y: f64,
        screen_width: f64,
        screen_height: f64,
        device_pixel_ratio: f64,
    ) {
        let anchors = SurfaceAnchors::from_bits(panel_anchors.cast_unsigned());
        let panel = PanelSize {
            width: panel_width,
            height: panel_height,
        };
        let screen = OutputGeometry {
            x: screen_x,
            y: screen_y,
            width: screen_width,
            height: screen_height,
            scale: device_pixel_ratio,
        };
        let anchor = Point::new(anchor_x, anchor_y);
        let pos = compute_menu_placement(
            anchors,
            panel,
            anchor,
            anchor_width,
            anchor_height,
            menu_width,
            menu_height,
            screen,
        );

        let item_id_str = item_id.to_string();
        self.as_mut().set_menu_x(pos.x);
        self.as_mut().set_menu_y(pos.y);
        self.as_mut().set_item_id(item_id.clone());

        {
            let mut rust = self.as_mut().rust_mut();
            rust.latency_tracker.mark_open_requested();
        }

        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            let _ = send_menu_about_to_show_call(&item_id_str, 0).await;

            match fetch_menu_layout(&item_id_str).await {
                Ok((rev, items)) => {
                    let _ = qt_thread.queue(move |mut menu| {
                        menu.as_mut().begin_reset_model();
                        {
                            let mut rust = menu.as_mut().rust_mut();
                            rust.set_items(items, rev);
                            let count = rust.count;
                            let sel = rust.selected_index;
                            let parent = rust.active_parent_id;
                            menu.as_mut().set_count(count);
                            menu.as_mut().set_selected_index(sel);
                            menu.as_mut().set_active_parent_id(parent);
                            menu.as_mut().set_visible(true);
                        }
                        menu.as_mut().end_reset_model();
                    });
                }
                Err(e) => {
                    warn!("Menu: Failed to get menu for '{item_id_str}': {e}");
                }
            }
        });
    }

    /// Closes the menu popup.
    pub fn close(mut self: Pin<&mut Self>) {
        self.as_mut().begin_reset_model();
        self.as_mut().set_visible(false);
        self.as_mut().rust_mut().clear();
        self.as_mut().set_count(0);
        self.as_mut().set_selected_index(-1);
        self.as_mut().set_active_parent_id(0);
        self.as_mut().end_reset_model();
    }

    /// Activates an item by menu item id (sends MenuEvent with layout revision then closes).
    pub fn activate_item(mut self: Pin<&mut Self>, menu_item_id: i32) {
        let item_id_str = self.as_ref().item_id().to_string();
        let rev = self.as_ref().rust().revision;
        self.as_mut().close();

        tokio::spawn(async move {
            let _ = send_menu_event_call(&item_id_str, menu_item_id, "clicked", rev).await;
        });
    }

    /// Navigates selection to the next enabled, non-separator item.
    pub fn select_next(mut self: Pin<&mut Self>) {
        let sel = self.as_mut().rust_mut().select_next();
        self.as_mut().set_selected_index(sel);
    }

    /// Navigates selection to the previous enabled, non-separator item.
    pub fn select_prev(mut self: Pin<&mut Self>) {
        let sel = self.as_mut().rust_mut().select_prev();
        self.as_mut().set_selected_index(sel);
    }

    /// Navigates selection to the first enabled item.
    pub fn select_first(mut self: Pin<&mut Self>) {
        let sel = self.as_mut().rust_mut().select_first();
        self.as_mut().set_selected_index(sel);
    }

    /// Navigates selection to the last enabled item.
    pub fn select_last(mut self: Pin<&mut Self>) {
        let sel = self.as_mut().rust_mut().select_last();
        self.as_mut().set_selected_index(sel);
    }

    /// Drills down into the submenu of the currently selected item.
    pub fn open_submenu(mut self: Pin<&mut Self>) {
        let item_id_str = self.as_ref().item_id().to_string();
        let sub_opt = self.as_mut().rust_mut().open_submenu();

        if let Some(sub_id) = sub_opt {
            self.as_mut().begin_reset_model();
            let count = self.as_ref().rust().count;
            let sel = self.as_ref().rust().selected_index;
            self.as_mut().set_count(count);
            self.as_mut().set_selected_index(sel);
            self.as_mut().set_active_parent_id(sub_id);
            self.as_mut().end_reset_model();

            tokio::spawn(async move {
                let _ = send_menu_about_to_show_call(&item_id_str, sub_id).await;
            });
        }
    }

    /// Pops back up to the parent menu level, or closes menu if at root.
    pub fn close_submenu(mut self: Pin<&mut Self>) {
        let was_sub = self.as_mut().rust_mut().close_submenu();
        if was_sub {
            self.as_mut().begin_reset_model();
            let count = self.as_ref().rust().count;
            let sel = self.as_ref().rust().selected_index;
            let parent = self.as_ref().rust().active_parent_id;
            self.as_mut().set_count(count);
            self.as_mut().set_selected_index(sel);
            self.as_mut().set_active_parent_id(parent);
            self.as_mut().end_reset_model();
        } else {
            self.close();
        }
    }

    /// Activates the currently selected item.
    pub fn activate_selected(mut self: Pin<&mut Self>) {
        let act = self.as_ref().rust().activate_selected();
        if let Some((id, has_sub)) = act {
            if has_sub {
                self.as_mut().open_submenu();
            } else {
                self.as_mut().activate_item(id);
            }
        }
    }

    /// Records the first frame rendered after menu open request.
    pub fn record_first_frame(mut self: Pin<&mut Self>) {
        if let Some(elapsed) = self.as_mut().rust_mut().record_first_frame() {
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
            let frame_ns = crate::shim::ffi::kynex_steady_clock_ns();
            tracing::info!(
                target: "kynex_perf",
                event = "menu_first_frame_swapped",
                frame_ns = frame_ns,
                latency_ms = elapsed_ms,
                "Menu first frame swapped: frame_ns={frame_ns} shell_latency_ms={elapsed_ms:.2}"
            );
        }
    }
}

/// Runs background D-Bus listener monitoring `MenuChanged` signals and daemon disconnects.
async fn run_menu_listener(qt_thread: cxx_qt::CxxQtThread<qobject::TrayMenuModel>) {
    loop {
        let Ok(conn) = Connection::session().await else {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            continue;
        };

        let Ok(proxy) = Tray1Proxy::builder(&conn)
            .destination(DAEMON_DESTINATION)
            .path(TRAY_PATH)
            .build()
            .await
        else {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            continue;
        };

        let Ok(mut stream) = proxy.receive_menu_changed().await else {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            continue;
        };

        let dbus_proxy = DBusProxy::new(&conn).await.ok();
        let mut owner_stream = if let Some(dp) = dbus_proxy.as_ref() {
            dp.receive_name_owner_changed().await.ok()
        } else {
            None
        };

        if std::env::var("KYNEX_BENCH_OPEN_MENU").unwrap_or_default() == "1" {
            let proxy_sub = proxy.clone();
            let thread_sub = qt_thread.clone();
            tokio::spawn(async move {
                if let Ok(mut added_stream) = proxy_sub.receive_item_added().await {
                    while let Some(signal) = added_stream.next().await {
                        if let Ok(args) = signal.args() {
                            let item = args.item;
                            if item.is_menu {
                                let iid = QString::from(&item.id);
                                let _ = thread_sub.queue(move |mut menu| {
                                    menu.as_mut().open(
                                        &iid,
                                        100.0,
                                        32.0,
                                        24.0,
                                        24.0,
                                        200.0,
                                        300.0,
                                        2 | 4 | 8,
                                        1920.0,
                                        48.0,
                                        0.0,
                                        0.0,
                                        1920.0,
                                        1080.0,
                                        1.0,
                                    );
                                });
                            }
                        }
                    }
                }
            });

            let proxy_chg = proxy.clone();
            let thread_chg = qt_thread.clone();
            tokio::spawn(async move {
                if let Ok(mut changed_stream) = proxy_chg.receive_item_changed().await {
                    while let Some(signal) = changed_stream.next().await {
                        if let Ok(args) = signal.args() {
                            let item = args.item;
                            if item.is_menu {
                                let iid = QString::from(&item.id);
                                let _ = thread_chg.queue(move |mut menu| {
                                    menu.as_mut().open(
                                        &iid,
                                        100.0,
                                        32.0,
                                        24.0,
                                        24.0,
                                        200.0,
                                        300.0,
                                        2 | 4 | 8,
                                        1920.0,
                                        48.0,
                                        0.0,
                                        0.0,
                                        1920.0,
                                        1080.0,
                                        1.0,
                                    );
                                });
                            }
                        }
                    }
                }
            });
        }

        loop {
            tokio::select! {
                Some(signal) = stream.next() => {
                    if let Ok(args) = signal.args() {
                        let item_id = args.item_id.to_string();
                        let proxy_clone = proxy.clone();
                        let thread_clone = qt_thread.clone();

                        tokio::spawn(async move {
                            if let Ok((rev, items)) = proxy_clone.get_menu(&item_id).await {
                                let _ = thread_clone.queue(move |mut menu| {
                                    if *menu.as_ref().visible()
                                        && menu.as_ref().item_id().to_string() == item_id
                                    {
                                        menu.as_mut().begin_reset_model();
                                        {
                                            let mut rust = menu.as_mut().rust_mut();
                                            rust.set_items(items, rev);
                                            let count = rust.count;
                                            let sel = rust.selected_index;
                                            let parent = rust.active_parent_id;
                                            menu.as_mut().set_count(count);
                                            menu.as_mut().set_selected_index(sel);
                                            menu.as_mut().set_active_parent_id(parent);
                                        }
                                        menu.as_mut().end_reset_model();
                                    }
                                });
                            }
                        });
                    }
                }
                Some(owner_change) = async {
                    match owner_stream.as_mut() {
                        Some(s) => s.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Ok(args) = owner_change.args() {
                        if args.name == DAEMON_DESTINATION && args.new_owner.is_none() {
                            // Daemon disconnected: close menu overlay and reset model
                            let _ = qt_thread.queue(|mut menu| {
                                menu.as_mut().close();
                            });
                            break; // Reconnect outer loop
                        }
                    }
                }
                else => break,
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dto(id: i32, parent: i32, label: &str, enabled: bool, has_sub: bool) -> MenuItemDto {
        MenuItemDto {
            id,
            parent,
            label: label.to_string(),
            enabled,
            visible: true,
            kind: 0,
            toggle: 1,
            toggle_state: 1,
            icon_name: "test-icon".to_string(),
            disposition: 0,
            shortcut: "Ctrl+N".to_string(),
            has_submenu: has_sub,
        }
    }

    #[test]
    fn test_menu_model_rust_all_roles_and_data() {
        let mut rust = TrayMenuModelRust::default();
        assert_eq!(rust.row_count(), 0);
        assert!(!rust.visible);

        let items = vec![
            sample_dto(1, 0, "Item 1", true, false),
            sample_dto(2, 0, "Item 2", false, true),
        ];

        rust.set_items(items, 10);
        assert_eq!(rust.row_count(), 2);
        assert_eq!(rust.revision, 10);

        // Test all roles for item 0
        let v_id = rust.data(0, ROLE_MENU_ID);
        assert_eq!(v_id.value::<i32>().unwrap(), 1);

        let v_parent = rust.data(0, ROLE_MENU_PARENT_ID);
        assert_eq!(v_parent.value::<i32>().unwrap(), 0);

        let v_label = rust.data(0, ROLE_MENU_LABEL);
        assert_eq!(v_label.value::<QString>().unwrap().to_string(), "Item 1");

        let v_enabled = rust.data(0, ROLE_MENU_ENABLED);
        assert_eq!(v_enabled.value::<bool>().unwrap(), true);

        let v_visible = rust.data(0, ROLE_MENU_VISIBLE);
        assert_eq!(v_visible.value::<bool>().unwrap(), true);

        let v_kind = rust.data(0, ROLE_MENU_KIND);
        assert_eq!(v_kind.value::<i32>().unwrap(), 0);

        let v_toggle = rust.data(0, ROLE_MENU_TOGGLE);
        assert_eq!(v_toggle.value::<i32>().unwrap(), 1);

        let v_toggle_state = rust.data(0, ROLE_MENU_TOGGLE_STATE);
        assert_eq!(v_toggle_state.value::<i32>().unwrap(), 1);

        let v_icon = rust.data(0, ROLE_MENU_ICON_NAME);
        assert_eq!(v_icon.value::<QString>().unwrap().to_string(), "test-icon");

        let v_disp = rust.data(0, ROLE_MENU_DISPOSITION);
        assert_eq!(v_disp.value::<i32>().unwrap(), 0);

        let v_shortcut = rust.data(0, ROLE_MENU_SHORTCUT);
        assert_eq!(v_shortcut.value::<QString>().unwrap().to_string(), "Ctrl+N");

        let v_has_sub = rust.data(0, ROLE_MENU_HAS_SUBMENU);
        assert_eq!(v_has_sub.value::<bool>().unwrap(), false);

        // Out of bounds and invalid role
        assert!(rust.data(5, ROLE_MENU_ID).is_null());
        assert!(rust.data(-1, ROLE_MENU_ID).is_null());
        assert!(rust.data(0, 9999).is_null());

        // Clear
        rust.clear();
        assert_eq!(rust.row_count(), 0);
        assert_eq!(rust.revision, 0);
        assert_eq!(rust.selected_index, -1);
    }

    #[test]
    fn test_menu_model_rust_navigation_and_submenus() {
        let mut rust = TrayMenuModelRust::default();
        let items = vec![
            sample_dto(1, 0, "Item 1", true, false),
            sample_dto(2, 0, "Submenu 1", true, true),
            sample_dto(3, 2, "Sub-item A", true, false),
        ];

        rust.set_items(items, 1);
        assert_eq!(rust.selected_index, 0);

        // Select next -> item 1 (index 1)
        assert_eq!(rust.select_next(), 1);
        // Select next -> wrap around to 0
        assert_eq!(rust.select_next(), 0);
        // Select prev -> wrap to 1
        assert_eq!(rust.select_prev(), 1);
        // Select first
        assert_eq!(rust.select_first(), 0);
        // Select last
        assert_eq!(rust.select_last(), 1);

        // Open submenu for item 2
        assert_eq!(rust.open_submenu(), Some(2));
        assert_eq!(rust.active_parent_id, 2);
        assert_eq!(rust.row_count(), 1);

        // Close submenu -> back to root
        assert!(rust.close_submenu());
        assert_eq!(rust.active_parent_id, 0);
        assert_eq!(rust.row_count(), 2);

        // Close submenu at root returns false
        assert!(!rust.close_submenu());
    }

    #[test]
    fn test_menu_model_rust_activate_selected() {
        let mut rust = TrayMenuModelRust::default();
        let items = vec![
            sample_dto(10, 0, "Action", true, false),
            sample_dto(20, 0, "Folder", true, true),
        ];
        rust.set_items(items, 1);

        // First item is regular action
        rust.select_first();
        assert_eq!(rust.activate_selected(), Some((10, false)));

        // Second item is submenu
        rust.select_last();
        assert_eq!(rust.activate_selected(), Some((20, true)));
    }

    #[test]
    fn test_menu_model_rust_latency_tracking() {
        let mut rust = TrayMenuModelRust::default();
        assert!(rust.record_first_frame().is_none());

        rust.latency_tracker.mark_open_requested();
        let elapsed = rust.record_first_frame();
        assert!(elapsed.is_some());
        assert!(rust.record_first_frame().is_none());
    }

    #[test]
    fn test_create_menu_role_names() {
        let roles = create_menu_role_names();
        assert_eq!(roles.len(), 12);
        assert_eq!(roles.get(&ROLE_MENU_ID).unwrap().to_string(), "id");
        assert_eq!(roles.get(&ROLE_MENU_LABEL).unwrap().to_string(), "label");
        assert_eq!(
            roles.get(&ROLE_MENU_SHORTCUT).unwrap().to_string(),
            "shortcut"
        );
        assert_eq!(
            roles.get(&ROLE_MENU_HAS_SUBMENU).unwrap().to_string(),
            "hasSubmenu"
        );
    }

    #[test]
    fn test_menu_signal_parsing_and_filtering() {
        assert!(should_update_on_menu_changed(
            "test-item",
            "test-item",
            true
        ));
        assert!(!should_update_on_menu_changed(
            "test-item",
            "test-item",
            false
        ));
        assert!(!should_update_on_menu_changed(
            "test-item",
            "other-item",
            true
        ));

        let mut state = MenuCoreState::new();
        state.set_items(vec![sample_dto(1, 0, "A", true, false)]);
        assert_eq!(state.total_count(), 1);

        assert!(handle_menu_name_owner_changed(&mut state, None));
        assert_eq!(state.total_count(), 0);
    }

    #[tokio::test]
    async fn test_send_menu_calls_offline() {
        // Calling when disconnected must return Err without panicking
        let _ = send_menu_event_call("test", 1, "clicked", 1).await;
        let _ = send_menu_about_to_show_call("test", 1).await;
        let _ = fetch_menu_layout("test").await;
    }
}
