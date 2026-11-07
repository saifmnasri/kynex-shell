//! Sub-services provided by `kynex-daemon`.

pub mod apps;
pub mod network;
pub mod pins;
pub mod power;
pub mod shell;
pub mod windows;

pub use apps::{setup_apps_watcher, AppsServer, AppsSharedState, LAUNCH_RATE_LIMIT_PER_SEC};
pub use network::{
    map_connection_type, map_nm_state, read_network_snapshot, run_network_service,
    sanitize_signal_strength, sanitize_ssid, AccessPointProxy, ActiveConnectionProxy,
    NetworkManagerProxy, NetworkServer, NetworkSnapshot,
};
pub use pins::{PinsServer, PinsSharedState, MAX_PINS, PINS_MUTATION_RATE_LIMIT_PER_SEC};
pub use power::{
    read_device_snapshot, run_power_service, sanitize_percentage, sanitize_state, sanitize_time,
    PowerServer, PowerSnapshot, UPowerDeviceProxy,
};
pub use shell::{
    run_shell_kwin_watcher, ShellFeedServer, ShellServer, ShellSharedState,
    SHELL_FEED_RATE_LIMIT_PER_SEC,
};
pub use windows::{
    run_kwin_watcher, AllowAllSenderPolicy, KWinSenderPolicy, SenderPolicy, WindowsFeedServer,
    WindowsServer, WindowsSharedState, FEED_RATE_LIMIT_PER_SEC,
};
