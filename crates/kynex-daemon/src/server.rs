//! D-Bus object implementation for `org.kynexos.Daemon.Core1`.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use kynex_ipc::DaemonStatus;

/// Root daemon D-Bus object served at `/org/kynexos/Daemon`.
#[derive(Debug)]
pub struct DaemonServer {
    start_time: Instant,
    #[allow(dead_code)]
    running: Arc<AtomicBool>,
}

impl DaemonServer {
    /// Creates a new `DaemonServer` instance.
    #[must_use]
    pub fn new(running: Arc<AtomicBool>) -> Self {
        Self {
            start_time: Instant::now(),
            running,
        }
    }
}

#[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
#[zbus::interface(name = "org.kynexos.Daemon.Core1")]
impl DaemonServer {
    /// Responds with "pong" to verify IPC connectivity.
    pub async fn ping(&self) -> &str {
        "pong"
    }

    /// Returns the daemon software version.
    pub async fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    /// Returns structured status information about the daemon and running services.
    pub async fn status(&self) -> DaemonStatus {
        DaemonStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            active_services: Vec::new(),
        }
    }
}
