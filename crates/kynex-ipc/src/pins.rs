//! Definitions for `org.kynexos.Daemon.Pins1` D-Bus interface.

/// Interface name for the pinned applications service.
pub const PINS_INTERFACE: &str = "org.kynexos.Daemon.Pins1";

/// Canonical D-Bus object path for pinned applications service.
pub const PINS_PATH: &str = "/org/kynexos/Daemon/Pins";

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Pins1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Pins1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Pins"
)]
pub trait Pins1 {
    /// Returns the ordered list of pinned application desktop IDs.
    fn get(&self) -> zbus::Result<Vec<String>>;

    /// Pins the specified application ID.
    fn pin(&self, id: &str) -> zbus::Result<()>;

    /// Unpins the specified application ID.
    fn unpin(&self, id: &str) -> zbus::Result<()>;

    /// Reorders the specified pinned application ID to `index`.
    #[zbus(name = "Move")]
    fn move_pin(&self, id: &str, index: u32) -> zbus::Result<()>;

    /// Signal emitted when the pinned applications list changes.
    #[zbus(signal)]
    fn changed(&self) -> zbus::Result<()>;
}
