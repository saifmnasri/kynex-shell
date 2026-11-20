//! Definitions for `org.kynexos.Shell.Debug1` D-Bus interface.

use zbus::proxy;

/// Canonical D-Bus object path for shell debug interface.
pub const SHELL_DEBUG_PATH: &str = "/org/kynexos/Shell";

/// The secondary object path for shell debug interface.
pub const SHELL_DEBUG_SUBPATH: &str = "/org/kynexos/Shell/Debug";

/// The well-known bus name for the shell.
pub const SHELL_DESTINATION: &str = "org.kynexos.Shell";

/// Interface name for `org.kynexos.Shell.Debug1`.
pub const SHELL_DEBUG_INTERFACE: &str = "org.kynexos.Shell.Debug1";

/// Client proxy for `org.kynexos.Shell.Debug1`.
#[proxy(
    interface = "org.kynexos.Shell.Debug1",
    default_service = "org.kynexos.Shell",
    default_path = "/org/kynexos/Shell"
)]
pub trait Debug1 {
    /// Returns a sanitized JSON dump of shell state.
    ///
    /// Never contains window titles, notification bodies, or other user content.
    fn dump_state(&self) -> zbus::Result<String>;

    /// Whether all primary shell data models are initialized and populated.
    #[zbus(property)]
    fn models_ready(&self) -> zbus::Result<bool>;
}
