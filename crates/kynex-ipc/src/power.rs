//! Definitions for `org.kynexos.Daemon.Power1` D-Bus interface.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// The interface name for the power management service.
pub const POWER_INTERFACE: &str = "org.kynexos.Daemon.Power1";

/// Canonical D-Bus object path for power management.
pub const POWER_PATH: &str = "/org/kynexos/Daemon";

/// Power device operational state matching `UPower` specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[repr(u32)]
pub enum BatteryState {
    /// State cannot be determined or no battery is present.
    #[default]
    Unknown = 0,
    /// Battery is charging from an external power supply.
    Charging = 1,
    /// Battery is discharging (operating on battery power).
    Discharging = 2,
    /// Battery charge is completely depleted.
    Empty = 3,
    /// Battery is connected to power and charged to capacity.
    FullyCharged = 4,
    /// Battery is connected to power and preparing to charge.
    PendingCharge = 5,
    /// Battery is preparing to discharge.
    PendingDischarge = 6,
}

impl BatteryState {
    /// Maps a raw u32 to a validated `BatteryState`. Unknown or invalid values map to `Unknown`.
    #[must_use]
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }

    /// Returns `true` if the battery is actively charging.
    #[must_use]
    pub fn is_charging(&self) -> bool {
        matches!(self, Self::Charging | Self::PendingCharge)
    }
}

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Power1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Power1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon"
)]
pub trait Power1 {
    /// Whether a battery device is present.
    #[zbus(property)]
    fn present(&self) -> zbus::Result<bool>;

    /// Current battery charge percentage (clamped 0.0 to 100.0).
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    /// Battery state code (0=Unknown, 1=Charging, 2=Discharging, etc.).
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// Estimated seconds remaining until battery is empty.
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;

    /// Estimated seconds remaining until battery is fully charged.
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
}
