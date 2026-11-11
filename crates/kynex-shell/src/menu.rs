//! Shell UI model and CXX-Qt bridge for system tray dbusmenu presentation.
//!
//! Enforces:
//! - Non-blocking asynchronous interaction: fetching and events never block the Qt GUI thread.
//! - Pure state delegation to [`MenuCoreState`] in `kynex-shell-core`.
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
use std::time::Instant;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use kynex_ipc::tray::{MenuItemDto, Tray1Proxy, TRAY_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use kynex_shell_core::coordinates::{OutputGeometry, Point};
use kynex_shell_core::menu::{
    compute_menu_popup_geometry, menu_role_names, MenuCoreState, ROLE_MENU_DISPOSITION,
    ROLE_MENU_ENABLED, ROLE_MENU_HAS_SUBMENU, ROLE_MENU_ICON_NAME, ROLE_MENU_ID, ROLE_MENU_KIND,
    ROLE_MENU_LABEL, ROLE_MENU_PARENT_ID, ROLE_MENU_SHORTCUT, ROLE_MENU_TOGGLE,
    ROLE_MENU_TOGGLE_STATE, ROLE_MENU_VISIBLE,
};
use tracing::{info, warn};
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
    match role {
        ROLE_MENU_ID => QVariant::from(&item.id),
        ROLE_MENU_PARENT_ID => QVariant::from(&item.parent),
        ROLE_MENU_LABEL => QVariant::from(&QString::from(&item.label)),
        ROLE_MENU_ENABLED => QVariant::from(&item.enabled),
        ROLE_MENU_VISIBLE => QVariant::from(&item.visible),
        ROLE_MENU_KIND => QVariant::from(&(item.kind as i32)),
        ROLE_MENU_TOGGLE => QVariant::from(&(item.toggle as i32)),
        ROLE_MENU_TOGGLE_STATE => QVariant::from(&item.toggle_state),
        ROLE_MENU_ICON_NAME => QVariant::from(&QString::from(&item.icon_name)),
        ROLE_MENU_DISPOSITION => QVariant::from(&(item.disposition as i32)),
        ROLE_MENU_SHORTCUT => QVariant::from(&QString::from(&item.shortcut)),
        ROLE_MENU_HAS_SUBMENU => QVariant::from(&item.has_submenu),
        _ => QVariant::default(),
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

        /// Initializes background D-Bus listeners and monitor daemon lifecycle.
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

        /// Opens the menu for a tray item at anchor location.
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
            screen_x: f64,
            screen_y: f64,
            screen_width: f64,
            screen_height: f64,
        );

        /// Closes the menu popup.
        #[qinvokable]
        fn close(self: Pin<&mut Self>);

        /// Activates an item by menu item id (sends MenuEvent then closes).
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
    pub(crate) first_frame_pending: bool,
    pub(crate) last_open_time: Option<Instant>,
}

impl TrayMenuModelRust {
    /// Returns the number of items in the current active menu level.
    #[must_use]
    pub fn row_count(&self) -> i32 {
        self.count
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

    /// Records the first frame rendered after opening the menu.
    pub fn record_first_frame(&mut self) -> Option<f64> {
        if self.first_frame_pending {
            self.first_frame_pending = false;
            let frame_ns = crate::shim::ffi::kynex_steady_clock_ns();
            let elapsed_ms = self
                .last_open_time
                .map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0);
            info!(
                target: "kynex_perf",
                event = "menu_first_frame_swapped",
                frame_ns = frame_ns,
                latency_ms = elapsed_ms,
                "Menu first frame swapped: frame_ns={frame_ns} shell_latency_ms={elapsed_ms:.2}"
            );
            Some(elapsed_ms)
        } else {
            None
        }
    }
}

impl qobject::TrayMenuModel {
    /// Initializes background listeners.
    pub fn init(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_menu_listener(qt_thread).await;
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
        create_menu_role_names()
    }

    /// Opens the menu for a tray item at anchor location.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        mut self: Pin<&mut Self>,
        item_id: &QString,
        anchor_x: f64,
        anchor_y: f64,
        anchor_width: f64,
        anchor_height: f64,
        menu_width: f64,
        menu_height: f64,
        screen_x: f64,
        screen_y: f64,
        screen_width: f64,
        screen_height: f64,
    ) {
        let screen = OutputGeometry {
            x: screen_x,
            y: screen_y,
            width: screen_width,
            height: screen_height,
            scale: 1.0,
        };
        let anchor = Point::new(anchor_x, anchor_y);
        let pos = compute_menu_popup_geometry(
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

        let mut rust = self.as_mut().rust_mut();
        rust.last_open_time = Some(Instant::now());
        rust.first_frame_pending = true;

        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            let Ok(conn) = Connection::session().await else {
                warn!("Menu: Failed to connect to session bus");
                return;
            };

            let Ok(proxy) = Tray1Proxy::builder(&conn)
                .destination(DAEMON_DESTINATION)
                .path(TRAY_PATH)
                .build()
                .await
            else {
                warn!("Menu: Failed to build Tray1Proxy");
                return;
            };

            // Call MenuAboutToShow(item_id, 0)
            let _ = proxy.menu_about_to_show(&item_id_str, 0).await;

            // Call GetMenu(item_id)
            match proxy.get_menu(&item_id_str).await {
                Ok((_rev, items)) => {
                    let _ = qt_thread.queue(move |mut menu| {
                        menu.as_mut().begin_reset_model();
                        let mut rust = menu.as_mut().rust_mut();
                        rust.state.set_items(items);
                        let count = i32::try_from(rust.state.active_items().len()).unwrap_or(0);
                        let sel = rust
                            .state
                            .selected_index()
                            .and_then(|i| i32::try_from(i).ok())
                            .unwrap_or(-1);
                        let parent = rust.state.current_parent_id();
                        menu.as_mut().set_count(count);
                        menu.as_mut().set_selected_index(sel);
                        menu.as_mut().set_active_parent_id(parent);
                        menu.as_mut().set_visible(true);
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
        self.as_mut().rust_mut().state.clear();
        self.as_mut().set_count(0);
        self.as_mut().set_selected_index(-1);
        self.as_mut().set_active_parent_id(0);
        self.as_mut().end_reset_model();
    }

    /// Activates an item by menu item id (sends MenuEvent then closes).
    pub fn activate_item(mut self: Pin<&mut Self>, menu_item_id: i32) {
        let item_id_str = self.as_ref().item_id().to_string();
        self.as_mut().close();

        tokio::spawn(async move {
            let Ok(conn) = Connection::session().await else {
                return;
            };
            if let Ok(proxy) = Tray1Proxy::builder(&conn)
                .destination(DAEMON_DESTINATION)
                .path(TRAY_PATH)
                .build()
                .await
            {
                let _ = proxy
                    .menu_event(&item_id_str, menu_item_id, "clicked")
                    .await;
            }
        });
    }

    /// Navigates selection to the next enabled, non-separator item.
    pub fn select_next(mut self: Pin<&mut Self>) {
        let mut rust = self.as_mut().rust_mut();
        rust.state.select_next();
        let sel = rust
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.as_mut().set_selected_index(sel);
    }

    /// Navigates selection to the previous enabled, non-separator item.
    pub fn select_prev(mut self: Pin<&mut Self>) {
        let mut rust = self.as_mut().rust_mut();
        rust.state.select_prev();
        let sel = rust
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.as_mut().set_selected_index(sel);
    }

    /// Navigates selection to the first enabled item.
    pub fn select_first(mut self: Pin<&mut Self>) {
        let mut rust = self.as_mut().rust_mut();
        rust.state.select_first();
        let sel = rust
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.as_mut().set_selected_index(sel);
    }

    /// Navigates selection to the last enabled item.
    pub fn select_last(mut self: Pin<&mut Self>) {
        let mut rust = self.as_mut().rust_mut();
        rust.state.select_last();
        let sel = rust
            .state
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        self.as_mut().set_selected_index(sel);
    }

    /// Drills down into the submenu of the currently selected item.
    pub fn open_submenu(mut self: Pin<&mut Self>) {
        let item_id_str = self.as_ref().item_id().to_string();
        let submenu_id_opt = {
            let mut rust = self.as_mut().rust_mut();
            rust.state.enter_submenu()
        };

        if let Some(sub_id) = submenu_id_opt {
            self.as_mut().begin_reset_model();
            let count = i32::try_from(self.as_ref().rust().state.active_items().len()).unwrap_or(0);
            let sel = self
                .as_ref()
                .rust()
                .state
                .selected_index()
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1);
            self.as_mut().set_count(count);
            self.as_mut().set_selected_index(sel);
            self.as_mut().set_active_parent_id(sub_id);
            self.as_mut().end_reset_model();

            tokio::spawn(async move {
                let Ok(conn) = Connection::session().await else {
                    return;
                };
                if let Ok(proxy) = Tray1Proxy::builder(&conn)
                    .destination(DAEMON_DESTINATION)
                    .path(TRAY_PATH)
                    .build()
                    .await
                {
                    let _ = proxy.menu_about_to_show(&item_id_str, sub_id).await;
                }
            });
        }
    }

    /// Pops back up to the parent menu level, or closes menu if at root.
    pub fn close_submenu(mut self: Pin<&mut Self>) {
        let was_submenu = {
            let mut rust = self.as_mut().rust_mut();
            rust.state.leave_submenu()
        };

        if was_submenu {
            self.as_mut().begin_reset_model();
            let count = i32::try_from(self.as_ref().rust().state.active_items().len()).unwrap_or(0);
            let sel = self
                .as_ref()
                .rust()
                .state
                .selected_index()
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1);
            let parent = self.as_ref().rust().state.current_parent_id();
            self.as_mut().set_count(count);
            self.as_mut().set_selected_index(sel);
            self.as_mut().set_active_parent_id(parent);
            self.as_mut().end_reset_model();
        } else {
            // Already at root, close menu
            self.close();
        }
    }

    /// Activates the currently selected item.
    pub fn activate_selected(mut self: Pin<&mut Self>) {
        let selected_item = self.as_ref().rust().state.selected_item().cloned();
        if let Some(item) = selected_item {
            if item.has_submenu {
                self.as_mut().open_submenu();
            } else if item.kind == 0 && item.enabled {
                self.as_mut().activate_item(item.id);
            }
        }
    }

    /// Records the first frame rendered after menu open request.
    pub fn record_first_frame(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().record_first_frame();
    }
}

/// Runs background D-Bus listener monitoring `MenuChanged` signals from `kynex-daemon`.
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

        if std::env::var("KYNEX_BENCH_OPEN_MENU").unwrap_or_default() == "1" {
            let proxy_sub = proxy.clone();
            let thread_sub = qt_thread.clone();
            tokio::spawn(async move {
                if let Ok(mut added_stream) = proxy_sub.receive_item_added().await {
                    while let Some(signal) = futures_util::StreamExt::next(&mut added_stream).await
                    {
                        if let Ok(args) = signal.args() {
                            let item = args.item;
                            if item.is_menu {
                                let iid = QString::from(&item.id);
                                let _ = thread_sub.queue(move |mut menu| {
                                    menu.as_mut().open(
                                        &iid, 100.0, 32.0, 24.0, 24.0, 200.0, 300.0, 0.0, 0.0,
                                        1920.0, 1080.0,
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
                    while let Some(signal) =
                        futures_util::StreamExt::next(&mut changed_stream).await
                    {
                        if let Ok(args) = signal.args() {
                            let item = args.item;
                            if item.is_menu {
                                let iid = QString::from(&item.id);
                                let _ = thread_chg.queue(move |mut menu| {
                                    menu.as_mut().open(
                                        &iid, 100.0, 32.0, 24.0, 24.0, 200.0, 300.0, 0.0, 0.0,
                                        1920.0, 1080.0,
                                    );
                                });
                            }
                        }
                    }
                }
            });
        }

        while let Some(signal) = futures_util::StreamExt::next(&mut stream).await {
            if let Ok(args) = signal.args() {
                let item_id = args.item_id.to_string();
                let proxy_clone = proxy.clone();
                let thread_clone = qt_thread.clone();

                tokio::spawn(async move {
                    if let Ok((_rev, items)) = proxy_clone.get_menu(&item_id).await {
                        let _ = thread_clone.queue(move |mut menu| {
                            if *menu.as_ref().visible()
                                && menu.as_ref().item_id().to_string() == item_id
                            {
                                menu.as_mut().begin_reset_model();
                                let mut rust = menu.as_mut().rust_mut();
                                rust.state.set_items(items);
                                let count =
                                    i32::try_from(rust.state.active_items().len()).unwrap_or(0);
                                let sel = rust
                                    .state
                                    .selected_index()
                                    .and_then(|i| i32::try_from(i).ok())
                                    .unwrap_or(-1);
                                let parent = rust.state.current_parent_id();
                                menu.as_mut().set_count(count);
                                menu.as_mut().set_selected_index(sel);
                                menu.as_mut().set_active_parent_id(parent);
                                menu.as_mut().end_reset_model();
                            }
                        });
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_model_rust_basic() {
        let mut rust = TrayMenuModelRust::default();
        assert_eq!(rust.row_count(), 0);
        assert!(!rust.visible);
        assert!(rust.record_first_frame().is_none());
    }

    #[test]
    fn test_menu_role_variant() {
        let item = MenuItemDto {
            id: 42,
            parent: 0,
            label: "Open".to_string(),
            enabled: true,
            visible: true,
            kind: 0,
            toggle: 1,
            toggle_state: 1,
            icon_name: "document-open".to_string(),
            disposition: 0,
            shortcut: "Ctrl+O".to_string(),
            has_submenu: false,
        };

        let v_id = menu_item_role_variant(&item, ROLE_MENU_ID);
        assert_eq!(v_id.value::<i32>().unwrap(), 42);

        let v_label = menu_item_role_variant(&item, ROLE_MENU_LABEL);
        assert_eq!(v_label.value::<QString>().unwrap().to_string(), "Open");

        let v_en = menu_item_role_variant(&item, ROLE_MENU_ENABLED);
        assert_eq!(v_en.value::<bool>().unwrap(), true);

        let role_names = create_menu_role_names();
        assert_eq!(role_names.len(), 12);
        assert_eq!(role_names.get(&ROLE_MENU_ID).unwrap().to_string(), "id");
        assert_eq!(
            role_names.get(&ROLE_MENU_LABEL).unwrap().to_string(),
            "label"
        );
    }
}
