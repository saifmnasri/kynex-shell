//! Sub-services provided by `kynex-daemon`.

pub mod power;

pub use power::{
    read_device_snapshot, run_power_service, sanitize_percentage, sanitize_state, sanitize_time,
    PowerServer, PowerSnapshot, UPowerDeviceProxy,
};
