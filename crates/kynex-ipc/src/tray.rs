//! D-Bus interface definitions and client proxies for the System Tray subsystem:
//! - `org.kynexos.Daemon.Tray1`
//! - `org.kde.StatusNotifierWatcher` / `org.freedesktop.StatusNotifierWatcher`
//! - `org.kde.StatusNotifierItem`
#![allow(clippy::doc_markdown)]

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Interface name for internal shell tray service.
pub const TRAY_INTERFACE: &str = "org.kynexos.Daemon.Tray1";

/// Canonical D-Bus object path for internal shell tray service.
pub const TRAY_PATH: &str = "/org/kynexos/Daemon/Tray";

/// Well-known bus name for KDE StatusNotifierWatcher service.
pub const STATUS_NOTIFIER_WATCHER_DESTINATION_KDE: &str = "org.kde.StatusNotifierWatcher";

/// Well-known bus name for Freedesktop StatusNotifierWatcher service.
pub const STATUS_NOTIFIER_WATCHER_DESTINATION_FDO: &str = "org.freedesktop.StatusNotifierWatcher";

/// Canonical object path for StatusNotifierWatcher service.
pub const STATUS_NOTIFIER_WATCHER_PATH: &str = "/StatusNotifierWatcher";

/// Standard interface name for StatusNotifierWatcher.
pub const STATUS_NOTIFIER_WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";

/// Freedesktop interface name for StatusNotifierWatcher.
pub const STATUS_NOTIFIER_WATCHER_FDO_INTERFACE: &str = "org.freedesktop.StatusNotifierWatcher";

/// Standard interface name for StatusNotifierItem.
pub const STATUS_NOTIFIER_ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";

/// Freedesktop interface name for StatusNotifierItem.
pub const STATUS_NOTIFIER_ITEM_FDO_INTERFACE: &str = "org.freedesktop.StatusNotifierItem";

/// Default object path for StatusNotifierItem instances.
pub const STATUS_NOTIFIER_ITEM_DEFAULT_PATH: &str = "/StatusNotifierItem";

/// Representation of an active tray item exposed to `kynex-shell` over `Tray1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
pub struct TrayItemInfo {
    /// Unique item identifier string (e.g. `":1.42/StatusNotifierItem"`).
    pub id: String,
    /// Sanitized item title.
    pub title: String,
    /// Operational status (`"Passive"`, `"Active"`, `"NeedsAttention"`).
    pub status: String,
    /// Sanitized icon name conforming to Freedesktop icon theme specification.
    pub icon_name: String,
    /// Optional extra icon theme search path.
    pub icon_theme_path: String,
    /// Validated raw icon pixmap in RGBA format (empty if not provided).
    pub icon_pixmap: Vec<u8>,
    /// Width of `icon_pixmap` (0 if no pixmap).
    pub icon_pixmap_width: i32,
    /// Height of `icon_pixmap` (0 if no pixmap).
    pub icon_pixmap_height: i32,
    /// Sanitized tooltip headline.
    pub tooltip_title: String,
    /// Sanitized tooltip body description.
    pub tooltip_text: String,
    /// Whether item is menu-only.
    pub is_menu: bool,
    /// Window ID if associated with a window.
    pub window_id: i32,
    /// Whether item has requested user attention.
    pub has_attention: bool,
    /// Sanitized attention icon name.
    pub attention_icon_name: String,
    /// Validated attention icon pixmap in RGBA format (empty if not provided).
    pub attention_pixmap: Vec<u8>,
    /// Width of `attention_pixmap`.
    pub attention_pixmap_width: i32,
    /// Height of `attention_pixmap`.
    pub attention_pixmap_height: i32,
}

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Tray1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Tray1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Tray"
)]
pub trait Tray1 {
    /// Returns the full list of active tray items.
    fn get_items(&self) -> zbus::Result<Vec<TrayItemInfo>>;

    /// Invokes primary activation on target item.
    fn activate(&self, id: &str, x: i32, y: i32) -> zbus::Result<()>;

    /// Invokes secondary activation on target item.
    fn secondary_activate(&self, id: &str, x: i32, y: i32) -> zbus::Result<()>;

    /// Invokes context menu on target item with window coordinates.
    fn context_menu(&self, id: &str, x: i32, y: i32) -> zbus::Result<()>;

    /// Invokes scroll action on target item.
    fn scroll(&self, id: &str, delta: i32, orientation: &str) -> zbus::Result<()>;

    /// Signal emitted when a new tray item is added.
    #[zbus(signal)]
    fn item_added(&self, item: TrayItemInfo) -> zbus::Result<()>;

    /// Signal emitted when an existing tray item changes.
    #[zbus(signal)]
    fn item_changed(&self, item: TrayItemInfo) -> zbus::Result<()>;

    /// Signal emitted when a tray item is unregistered or removed.
    #[zbus(signal)]
    fn item_removed(&self, id: &str) -> zbus::Result<()>;
}

/// Client-side D-Bus proxy for `org.kde.StatusNotifierWatcher`.
#[zbus::proxy(
    interface = "org.kde.StatusNotifierWatcher",
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher"
)]
pub trait StatusNotifierWatcher {
    /// Registers an application's StatusNotifierItem instance.
    fn register_status_notifier_item(&self, service: &str) -> zbus::Result<()>;

    /// Registers a visualization panel host.
    fn register_status_notifier_host(&self, service: &str) -> zbus::Result<()>;

    /// Array of currently registered item identifier paths.
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> zbus::Result<Vec<String>>;

    /// Whether at least one visualization host is currently active.
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> zbus::Result<bool>;

    /// Implemented protocol version.
    #[zbus(property)]
    fn protocol_version(&self) -> zbus::Result<i32>;

    /// Signal emitted when a new item is registered.
    #[zbus(signal)]
    fn status_notifier_item_registered(&self, service: &str) -> zbus::Result<()>;

    /// Signal emitted when an item is unregistered.
    #[zbus(signal)]
    fn status_notifier_item_unregistered(&self, service: &str) -> zbus::Result<()>;

    /// Signal emitted when a new host registers.
    #[zbus(signal)]
    fn status_notifier_host_registered(&self) -> zbus::Result<()>;

    /// Signal emitted when the last host unregisters.
    #[zbus(signal)]
    fn status_notifier_host_unregistered(&self) -> zbus::Result<()>;
}

/// Freedesktop StatusNotifierWatcher module scoping proxy types.
pub mod fdo {
    /// Client-side D-Bus proxy for `org.freedesktop.StatusNotifierWatcher`.
    #[zbus::proxy(
        interface = "org.freedesktop.StatusNotifierWatcher",
        default_service = "org.freedesktop.StatusNotifierWatcher",
        default_path = "/StatusNotifierWatcher"
    )]
    pub trait FdoStatusNotifierWatcher {
        /// Registers an application's StatusNotifierItem instance.
        fn register_status_notifier_item(&self, service: &str) -> zbus::Result<()>;

        /// Registers a visualization panel host.
        fn register_status_notifier_host(&self, service: &str) -> zbus::Result<()>;

        /// Array of currently registered item identifier paths.
        #[zbus(property)]
        fn registered_status_notifier_items(&self) -> zbus::Result<Vec<String>>;

        /// Whether at least one visualization host is currently active.
        #[zbus(property)]
        fn is_status_notifier_host_registered(&self) -> zbus::Result<bool>;

        /// Implemented protocol version.
        #[zbus(property)]
        fn protocol_version(&self) -> zbus::Result<i32>;

        /// Signal emitted when a new item is registered.
        #[zbus(signal)]
        fn status_notifier_item_registered(&self, service: &str) -> zbus::Result<()>;

        /// Signal emitted when an item is unregistered.
        #[zbus(signal)]
        fn status_notifier_item_unregistered(&self, service: &str) -> zbus::Result<()>;

        /// Signal emitted when a new host registers.
        #[zbus(signal)]
        fn status_notifier_host_registered(&self) -> zbus::Result<()>;

        /// Signal emitted when the last host unregisters.
        #[zbus(signal)]
        fn status_notifier_host_unregistered(&self) -> zbus::Result<()>;
    }
}

/// Client-side D-Bus proxy for target `StatusNotifierItem` instances.
#[zbus::proxy(interface = "org.kde.StatusNotifierItem")]
pub trait StatusNotifierItem {
    /// Context menu interaction method.
    fn context_menu(&self, x: i32, y: i32) -> zbus::Result<()>;

    /// Primary activation interaction method.
    fn activate(&self, x: i32, y: i32) -> zbus::Result<()>;

    /// Secondary activation interaction method.
    fn secondary_activate(&self, x: i32, y: i32) -> zbus::Result<()>;

    /// Scroll interaction method.
    fn scroll(&self, delta: i32, orientation: &str) -> zbus::Result<()>;

    /// Category property.
    #[zbus(property)]
    fn category(&self) -> zbus::Result<String>;

    /// Identifier property.
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;

    /// Title property.
    #[zbus(property)]
    fn title(&self) -> zbus::Result<String>;

    /// Status property (`"Passive"`, `"Active"`, `"NeedsAttention"`).
    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;

    /// WindowId property.
    #[zbus(property)]
    fn window_id(&self) -> zbus::Result<i32>;

    /// IconThemePath property.
    #[zbus(property)]
    fn icon_theme_path(&self) -> zbus::Result<String>;

    /// IconName property.
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;

    /// IconPixmap property (`a(iiay)`).
    #[zbus(property)]
    fn icon_pixmap(&self) -> zbus::Result<Vec<(i32, i32, Vec<u8>)>>;

    /// OverlayIconName property.
    #[zbus(property)]
    fn overlay_icon_name(&self) -> zbus::Result<String>;

    /// OverlayIconPixmap property (`a(iiay)`).
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> zbus::Result<Vec<(i32, i32, Vec<u8>)>>;

    /// AttentionIconName property.
    #[zbus(property)]
    fn attention_icon_name(&self) -> zbus::Result<String>;

    /// AttentionIconPixmap property (`a(iiay)`).
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> zbus::Result<Vec<(i32, i32, Vec<u8>)>>;

    /// AttentionMovieName property.
    #[zbus(property)]
    fn attention_movie_name(&self) -> zbus::Result<String>;

    /// ToolTip property: `(icon_name, icon_pixmap, title, text)`.
    #[zbus(property)]
    fn tool_tip(&self) -> zbus::Result<(String, Vec<(i32, i32, Vec<u8>)>, String, String)>;

    /// ItemIsMenu property.
    #[zbus(property)]
    fn item_is_menu(&self) -> zbus::Result<bool>;

    /// Signal emitted when title changes.
    #[zbus(signal)]
    fn new_title(&self) -> zbus::Result<()>;

    /// Signal emitted when icon changes.
    #[zbus(signal)]
    fn new_icon(&self) -> zbus::Result<()>;

    /// Signal emitted when attention icon changes.
    #[zbus(signal)]
    fn new_attention_icon(&self) -> zbus::Result<()>;

    /// Signal emitted when overlay icon changes.
    #[zbus(signal)]
    fn new_overlay_icon(&self) -> zbus::Result<()>;

    /// Signal emitted when tooltip changes.
    #[zbus(signal)]
    fn new_tool_tip(&self) -> zbus::Result<()>;

    /// Signal emitted when status changes.
    #[zbus(signal)]
    fn new_status(&self, status: &str) -> zbus::Result<()>;
}
