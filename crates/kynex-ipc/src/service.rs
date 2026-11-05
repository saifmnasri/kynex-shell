//! In-process service abstractions for daemon modularity and fault isolation.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Type alias for a boxed future returned by asynchronous service lifecycle methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Operational health status of an in-process daemon service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ServiceHealth {
    /// Service is initializing or connecting to its underlying subsystem.
    Starting,
    /// Service is fully operational.
    Healthy,
    /// Subsystem service is unavailable or degraded (e.g. `UPower` missing),
    /// but the daemon remains alive.
    Degraded,
    /// Service has been stopped or shut down cleanly.
    Stopped,
}

/// Common interface for modular in-process daemon services.
///
/// Each subsystem (power, audio, networking, windows, etc.) implements this trait,
/// allowing independent initialization, fault tolerance, and clean shutdown.
pub trait Service: Send + Sync {
    /// Returns the unique machine-readable name of the service (e.g., "power", "audio").
    fn name(&self) -> &'static str;

    /// Starts or initializes the service.
    fn start(&mut self) -> BoxFuture<'_, Result<(), Box<dyn std::error::Error + Send + Sync>>>;

    /// Shuts down the service cleanly and releases resources.
    fn shutdown(&mut self) -> BoxFuture<'_, Result<(), Box<dyn std::error::Error + Send + Sync>>>;

    /// Queries the current health state of the service.
    fn health(&self) -> ServiceHealth;
}
