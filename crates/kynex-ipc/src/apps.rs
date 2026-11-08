//! Definitions for `org.kynexos.Daemon.Apps1` D-Bus interface.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Interface name for the applications indexing and launch service.
pub const APPS_INTERFACE: &str = "org.kynexos.Daemon.Apps1";

/// Canonical D-Bus object path for applications service.
pub const APPS_PATH: &str = "/org/kynexos/Daemon/Apps";

/// Representation of an application entry exposed over D-Bus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
pub struct AppItem {
    /// Desktop entry ID (e.g. `org.kde.konsole.desktop`).
    pub id: String,
    /// Localized application name.
    pub name: String,
    /// Localized generic name (e.g. `Terminal Emulator`).
    pub generic_name: String,
    /// Localized description/comment.
    pub comment: String,
    /// Icon name or path.
    pub icon_name: String,
    /// Categories list.
    pub categories: Vec<String>,
}

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Apps1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Apps1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Apps"
)]
pub trait Apps1 {
    /// Returns the full list of available applications.
    fn get_apps(&self) -> zbus::Result<Vec<AppItem>>;

    /// Searches and ranks applications matching `query` up to `limit` entries.
    fn search(&self, query: &str, limit: u32) -> zbus::Result<Vec<AppItem>>;

    /// Spawns the application binary associated with desktop entry `id`.
    fn launch(&self, id: &str) -> zbus::Result<()>;

    /// Signal emitted when application index changes across the filesystem.
    #[zbus(signal)]
    fn changed(&self) -> zbus::Result<()>;
}
