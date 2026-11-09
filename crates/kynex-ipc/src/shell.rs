//! Shell interface definitions for `org.kynexos.Daemon.Shell1` and `org.kynexos.Daemon.ShellFeed1`.

use zbus::proxy;

/// The D-Bus object path for the shell service.
pub const SHELL_PATH: &str = "/org/kynexos/Daemon/Shell";

/// The D-Bus interface name for `org.kynexos.Daemon.Shell1`.
pub const SHELL_INTERFACE: &str = "org.kynexos.Daemon.Shell1";

/// The D-Bus interface name for `org.kynexos.Daemon.ShellFeed1`.
pub const SHELL_FEED_INTERFACE: &str = "org.kynexos.Daemon.ShellFeed1";

/// Proxy for `org.kynexos.Daemon.Shell1`.
#[proxy(
    interface = "org.kynexos.Daemon.Shell1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Shell"
)]
pub trait Shell1 {
    /// Signal emitted when a launcher toggle is requested.
    #[zbus(signal)]
    fn launcher_toggle_requested(&self) -> zbus::Result<()>;
}

/// Proxy for `org.kynexos.Daemon.ShellFeed1`.
#[proxy(
    interface = "org.kynexos.Daemon.ShellFeed1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Shell"
)]
pub trait ShellFeed1 {
    /// Notifies daemon that the global launcher shortcut was triggered.
    fn launcher_shortcut(&self) -> zbus::Result<()>;
}
