//! Shared constants, configuration schemas, and common types for `KynexOS`.
//!
//! This crate contains pure data types and schemas with no direct I/O dependencies.

/// The standard D-Bus and application namespace for `KynexOS`.
pub const NAMESPACE: &str = "org.kynexos";

/// The default daemon service name.
pub const DAEMON_SERVICE_NAME: &str = "org.kynexos.Daemon";

/// The default style pack identifier.
pub const DEFAULT_STYLE_ID: &str = "default";
