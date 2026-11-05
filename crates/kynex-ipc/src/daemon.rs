//! Definitions for the root `org.kynexos.Daemon.Core1` D-Bus interface.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// The well-known D-Bus service name for the Kynex daemon.
pub const DAEMON_DESTINATION: &str = "org.kynexos.Daemon";

/// The canonical object path for the Kynex daemon.
pub const DAEMON_PATH: &str = "/org/kynexos/Daemon";

/// The interface name for the root daemon core interface (version 1).
pub const DAEMON_INTERFACE: &str = "org.kynexos.Daemon.Core1";

/// Current runtime status information emitted by the Kynex daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DaemonStatus {
    /// Version string of the daemon binary.
    pub version: String,
    /// Seconds elapsed since daemon startup.
    pub uptime_seconds: u64,
    /// Identifiers of currently active in-process services.
    pub active_services: Vec<String>,
}

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Core1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Core1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon"
)]
pub trait Core1 {
    /// Responds with "pong" to verify IPC connectivity.
    fn ping(&self) -> zbus::Result<String>;

    /// Returns the daemon software version.
    fn version(&self) -> zbus::Result<String>;

    /// Returns structured status information about the daemon and running services.
    fn status(&self) -> zbus::Result<DaemonStatus>;
}

/// Backward compatibility alias for [`Core1Proxy`].
pub use Core1Proxy as Daemon1Proxy;
