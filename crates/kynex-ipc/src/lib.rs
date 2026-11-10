//! D-Bus interface definitions and IPC protocols shared by the Kynex daemon and clients.

pub mod apps;
pub mod daemon;
pub mod network;
pub mod notifications;
pub mod pins;
pub mod power;
pub mod service;
pub mod shell;
pub mod windows;

pub use apps::{AppItem, AppMappingItem, Apps1Proxy, APPS_INTERFACE, APPS_PATH};
pub use daemon::{
    Core1Proxy, Daemon1Proxy, DaemonStatus, DAEMON_DESTINATION, DAEMON_INTERFACE, DAEMON_PATH,
};
pub use network::{Network1Proxy, NetworkKind, NetworkState, NETWORK_INTERFACE, NETWORK_PATH};
pub use notifications::{
    NotificationHistoryItem, Notifications1Proxy, FDO_NOTIFICATIONS_DESTINATION,
    FDO_NOTIFICATIONS_INTERFACE, FDO_NOTIFICATIONS_PATH, NOTIFICATIONS_INTERFACE,
    NOTIFICATIONS_PATH,
};
pub use pins::{Pins1Proxy, PINS_INTERFACE, PINS_PATH};
pub use power::{BatteryState, Power1Proxy, POWER_INTERFACE, POWER_PATH};
pub use service::{BoxFuture, Service, ServiceHealth};
pub use shell::{Shell1Proxy, ShellFeed1Proxy, SHELL_FEED_INTERFACE, SHELL_INTERFACE, SHELL_PATH};
pub use windows::{
    WindowItem, Windows1Proxy, WindowsFeed1Proxy, WINDOWS_FEED_INTERFACE, WINDOWS_INTERFACE,
    WINDOWS_PATH,
};

use kynex_common::NAMESPACE;

/// The base object path for `KynexOS` D-Bus objects.
pub const OBJECT_PATH_PREFIX: &str = "/org/kynexos";

/// Returns the formatted D-Bus interface name for a given interface suffix.
#[must_use]
pub fn interface_name(suffix: &str) -> String {
    format!("{NAMESPACE}.{suffix}")
}
