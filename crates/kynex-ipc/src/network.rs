//! Definitions for `org.kynexos.Daemon.Network1` D-Bus interface.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// The interface name for the network management service.
pub const NETWORK_INTERFACE: &str = "org.kynexos.Daemon.Network1";

/// Canonical D-Bus object path for network management.
pub const NETWORK_PATH: &str = "/org/kynexos/Daemon";

/// Network connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[repr(u32)]
pub enum NetworkState {
    /// Device is disconnected.
    Disconnected = 0,
    /// Connection is activating / connecting.
    Connecting = 1,
    /// Connection is fully activated and online.
    Connected = 2,
    /// Connection state is unknown or hardware is unmanaged.
    #[default]
    Unknown = 3,
}

impl NetworkState {
    /// Maps a raw u32 to a validated `NetworkState`.
    #[must_use]
    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => Self::Disconnected,
            1 => Self::Connecting,
            2 => Self::Connected,
            _ => Self::Unknown,
        }
    }
}

/// Network medium kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[repr(u32)]
pub enum NetworkKind {
    /// No network connection.
    #[default]
    None = 0,
    /// Wi-Fi (wireless 802.11) connection.
    Wifi = 1,
    /// Ethernet (wired 802.3) connection.
    Ethernet = 2,
    /// Other connection type (cellular, bluetooth, VPN, etc.).
    Other = 3,
}

impl NetworkKind {
    /// Maps a raw u32 to a validated `NetworkKind`.
    #[must_use]
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::Wifi,
            2 => Self::Ethernet,
            3 => Self::Other,
            _ => Self::None,
        }
    }
}

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Network1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Network1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon"
)]
pub trait Network1 {
    /// Whether network hardware/connectivity is managed and present.
    #[zbus(property)]
    fn present(&self) -> zbus::Result<bool>;

    /// Network operational state (0=Disconnected, 1=Connecting, 2=Connected, 3=Unknown).
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// Network connection medium (0=None, 1=Wifi, 2=Ethernet, 3=Other).
    #[zbus(property)]
    fn kind(&self) -> zbus::Result<u32>;

    /// Wi-Fi signal quality percentage (0 to 100).
    #[zbus(property)]
    fn signal_strength(&self) -> zbus::Result<u32>;

    /// Sanitized SSID string for active Wi-Fi connection.
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_state_mapping() {
        assert_eq!(NetworkState::from_u32(0), NetworkState::Disconnected);
        assert_eq!(NetworkState::from_u32(1), NetworkState::Connecting);
        assert_eq!(NetworkState::from_u32(2), NetworkState::Connected);
        assert_eq!(NetworkState::from_u32(3), NetworkState::Unknown);
        assert_eq!(NetworkState::from_u32(99), NetworkState::Unknown);
    }

    #[test]
    fn test_network_kind_mapping() {
        assert_eq!(NetworkKind::from_u32(0), NetworkKind::None);
        assert_eq!(NetworkKind::from_u32(1), NetworkKind::Wifi);
        assert_eq!(NetworkKind::from_u32(2), NetworkKind::Ethernet);
        assert_eq!(NetworkKind::from_u32(3), NetworkKind::Other);
        assert_eq!(NetworkKind::from_u32(42), NetworkKind::None);
    }
}
