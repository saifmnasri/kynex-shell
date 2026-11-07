//! Sub-services provided by `kynex-daemon`.

pub mod network;
pub mod power;
pub mod windows;

pub use network::{
    map_connection_type, map_nm_state, read_network_snapshot, run_network_service,
    sanitize_signal_strength, sanitize_ssid, AccessPointProxy, ActiveConnectionProxy,
    NetworkManagerProxy, NetworkServer, NetworkSnapshot,
};
pub use power::{
    read_device_snapshot, run_power_service, sanitize_percentage, sanitize_state, sanitize_time,
    PowerServer, PowerSnapshot, UPowerDeviceProxy,
};
pub use windows::{
    run_kwin_watcher, AllowAllSenderPolicy, KWinSenderPolicy, SenderPolicy, WindowsFeedServer,
    WindowsServer, WindowsSharedState, FEED_RATE_LIMIT_PER_SEC,
};
