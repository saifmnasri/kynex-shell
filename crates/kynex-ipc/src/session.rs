//! Definitions for `org.kynexos.Daemon.Session1` D-Bus interface.

/// The interface name for the session management service.
pub const SESSION_INTERFACE: &str = "org.kynexos.Daemon.Session1";

/// Canonical D-Bus object path for session management.
pub const SESSION_PATH: &str = "/org/kynexos/Daemon/Session";

/// Enforced global rate limit duration between power actions (seconds).
pub const SESSION_RATE_LIMIT_SECS: u64 = 5;

/// Enforced timeout duration on upstream IPC calls (seconds).
pub const SESSION_TIMEOUT_SECS: u64 = 2;

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Session1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Session1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Session"
)]
pub trait Session1 {
    /// Whether session logout is supported.
    #[zbus(property)]
    fn can_logout(&self) -> zbus::Result<bool>;

    /// Whether machine reboot/restart is supported and authorized.
    #[zbus(property)]
    fn can_restart(&self) -> zbus::Result<bool>;

    /// Whether machine power off / shut down is supported and authorized.
    #[zbus(property)]
    fn can_power_off(&self) -> zbus::Result<bool>;

    /// Whether machine suspend is supported and authorized.
    #[zbus(property)]
    fn can_suspend(&self) -> zbus::Result<bool>;

    /// Orders session logout by cleanly stopping `kynex-session.target`.
    fn logout(&self) -> zbus::Result<()>;

    /// Orders machine restart via logind `Reboot(interactive=false)`.
    fn restart(&self) -> zbus::Result<()>;

    /// Orders machine shutdown via logind `PowerOff(interactive=false)`.
    fn power_off(&self) -> zbus::Result<()>;

    /// Orders machine suspend via logind `Suspend(interactive=false)`.
    fn suspend(&self) -> zbus::Result<()>;

    /// Signal emitted when a session or power action fails or is refused.
    #[zbus(signal)]
    fn action_failed(&self, kind: String, reason: String) -> zbus::Result<()>;
}
