//! Definitions for `org.kynexos.Daemon.Notifications1` and `org.freedesktop.Notifications`.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Interface name for internal shell notifications interface.
pub const NOTIFICATIONS_INTERFACE: &str = "org.kynexos.Daemon.Notifications1";

/// Canonical D-Bus object path for internal shell notifications interface.
pub const NOTIFICATIONS_PATH: &str = "/org/kynexos/Daemon/Notifications";

/// Well-known bus name for standard Freedesktop notifications.
pub const FDO_NOTIFICATIONS_DESTINATION: &str = "org.freedesktop.Notifications";

/// Standard Freedesktop notifications interface.
pub const FDO_NOTIFICATIONS_INTERFACE: &str = "org.freedesktop.Notifications";

/// Standard Freedesktop notifications object path.
pub const FDO_NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

/// D-Bus representation of a notification history entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NotificationHistoryItem {
    /// Monotonic notification ID.
    pub id: u32,
    /// Sanitized display application name.
    pub app_name: String,
    /// Icon theme name or icon path.
    pub app_icon: String,
    /// Sanitized summary string.
    pub summary: String,
    /// Sanitized body string.
    pub body: String,
    /// Actions flattened as `[key1, label1, key2, label2, ...]`.
    pub actions: Vec<String>,
    /// Whether application identity was verified via desktop entry index.
    pub app_name_verified: bool,
    /// Urgency level (0=Low, 1=Normal, 2=Critical).
    pub urgency: u8,
}

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Notifications1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Notifications1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Notifications"
)]
pub trait Notifications1 {
    /// Dismisses notification with given ID.
    fn dismiss(&self, id: u32) -> zbus::Result<()>;

    /// Pauses expiration timeout for a visible notification popup (e.g. while hovered).
    fn pause_timeout(&self, id: u32) -> zbus::Result<()>;

    /// Resumes expiration timeout for a visible notification popup (e.g. when unhovered).
    fn resume_timeout(&self, id: u32) -> zbus::Result<()>;

    /// Invokes specified action key on notification with given ID.
    fn invoke_action(&self, id: u32, key: &str) -> zbus::Result<()>;

    /// Returns the bounded history list of past notifications.
    fn get_history(&self) -> zbus::Result<Vec<NotificationHistoryItem>>;

    /// Do Not Disturb mode property.
    #[zbus(property)]
    fn do_not_disturb(&self) -> zbus::Result<bool>;

    /// Sets Do Not Disturb mode property.
    #[zbus(property)]
    fn set_do_not_disturb(&self, value: bool) -> zbus::Result<()>;

    /// Signal emitted when a notification popup should be displayed.
    #[zbus(signal)]
    fn shown(
        &self,
        id: u32,
        app_name: String,
        app_name_verified: bool,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        urgency: u8,
        timeout_ms: i32,
    ) -> zbus::Result<()>;

    /// Signal emitted when an active notification popup is updated.
    #[zbus(signal)]
    fn updated(
        &self,
        id: u32,
        app_name: String,
        app_name_verified: bool,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        urgency: u8,
        timeout_ms: i32,
    ) -> zbus::Result<()>;

    /// Signal emitted when a notification popup is closed or removed.
    #[zbus(signal)]
    fn closed(&self, id: u32, reason: u32) -> zbus::Result<()>;
}
