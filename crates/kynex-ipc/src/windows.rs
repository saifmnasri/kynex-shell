//! Definitions for `org.kynexos.Daemon.WindowsFeed1` and `org.kynexos.Daemon.Windows1` D-Bus interfaces.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Interface for the private `KWin` window feeder.
pub const WINDOWS_FEED_INTERFACE: &str = "org.kynexos.Daemon.WindowsFeed1";

/// Interface for the public read-only window tracking service.
pub const WINDOWS_INTERFACE: &str = "org.kynexos.Daemon.Windows1";

/// Canonical D-Bus object path for window management.
pub const WINDOWS_PATH: &str = "/org/kynexos/Daemon/Windows";

/// Representation of a tracked window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
pub struct WindowItem {
    /// Window unique identifier (e.g. `KWin` `internalId`).
    pub id: String,
    /// Window title / caption (sanitized UTF-8).
    pub title: String,
    /// Window application ID / desktop file ID (sanitized).
    pub app_id: String,
    /// Whether the window is minimized.
    pub minimized: bool,
    /// Whether the window currently has active focus.
    pub active: bool,
}

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Windows1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Windows1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Windows"
)]
pub trait Windows1 {
    /// Returns the full list of currently tracked windows.
    fn list(&self) -> zbus::Result<Vec<WindowItem>>;

    /// Signal emitted when a new window is added.
    #[zbus(signal)]
    fn added(
        &self,
        id: &str,
        title: &str,
        app_id: &str,
        minimized: bool,
        active: bool,
    ) -> zbus::Result<()>;

    /// Signal emitted when window properties change.
    #[zbus(signal)]
    fn changed(
        &self,
        id: &str,
        title: &str,
        app_id: &str,
        minimized: bool,
        active: bool,
    ) -> zbus::Result<()>;

    /// Signal emitted when a window is removed.
    #[zbus(signal)]
    fn removed(&self, id: &str) -> zbus::Result<()>;

    /// Signal emitted when the active window changes.
    #[zbus(signal)]
    fn active_changed(&self, id: &str) -> zbus::Result<()>;
}

/// Client-side D-Bus proxy for `org.kynexos.Daemon.WindowsFeed1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.WindowsFeed1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Windows"
)]
pub trait WindowsFeed1 {
    /// Resets all tracked windows.
    fn reset(&self) -> zbus::Result<()>;

    /// Upserts a window.
    fn upsert(
        &self,
        id: &str,
        title: &str,
        app_id: &str,
        minimized: bool,
        active: bool,
    ) -> zbus::Result<()>;

    /// Removes a window.
    fn remove(&self, id: &str) -> zbus::Result<()>;

    /// Sets the active window.
    fn set_active(&self, id: &str) -> zbus::Result<()>;
}
