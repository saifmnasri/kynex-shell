//! D-Bus interface definitions and IPC protocols shared by the Kynex daemon and clients.

pub mod daemon;
pub mod power;
pub mod service;

pub use daemon::{
    Core1Proxy, Daemon1Proxy, DaemonStatus, DAEMON_DESTINATION, DAEMON_INTERFACE, DAEMON_PATH,
};
pub use power::{BatteryState, Power1Proxy, POWER_INTERFACE, POWER_PATH};
pub use service::{BoxFuture, Service, ServiceHealth};

use kynex_common::NAMESPACE;

/// The base object path for `KynexOS` D-Bus objects.
pub const OBJECT_PATH_PREFIX: &str = "/org/kynexos";

/// Returns the formatted D-Bus interface name for a given interface suffix.
#[must_use]
pub fn interface_name(suffix: &str) -> String {
    format!("{NAMESPACE}.{suffix}")
}
