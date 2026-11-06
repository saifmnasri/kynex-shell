//! Tests for kynex-ipc interfaces, constants, and data structures.

use kynex_ipc::{
    interface_name, BatteryState, DaemonStatus, ServiceHealth, DAEMON_DESTINATION,
    DAEMON_INTERFACE, DAEMON_PATH, OBJECT_PATH_PREFIX, POWER_INTERFACE, POWER_PATH,
};

#[test]
fn test_ipc_constants() {
    assert_eq!(DAEMON_DESTINATION, "org.kynexos.Daemon");
    assert_eq!(DAEMON_PATH, "/org/kynexos/Daemon");
    assert_eq!(DAEMON_INTERFACE, "org.kynexos.Daemon.Core1");
    assert_eq!(POWER_INTERFACE, "org.kynexos.Daemon.Power1");
    assert_eq!(POWER_PATH, "/org/kynexos/Daemon");
    assert_eq!(OBJECT_PATH_PREFIX, "/org/kynexos");
    assert_eq!(interface_name("Daemon.Core1"), "org.kynexos.Daemon.Core1");
    assert_eq!(interface_name("Power1"), "org.kynexos.Power1");
}

#[test]
fn test_battery_state_mapping() {
    assert_eq!(BatteryState::from_u32(0), BatteryState::Unknown);
    assert_eq!(BatteryState::from_u32(1), BatteryState::Charging);
    assert!(BatteryState::Charging.is_charging());
    assert_eq!(BatteryState::from_u32(2), BatteryState::Discharging);
    assert!(!BatteryState::Discharging.is_charging());
    assert_eq!(BatteryState::from_u32(3), BatteryState::Empty);
    assert_eq!(BatteryState::from_u32(4), BatteryState::FullyCharged);
    assert_eq!(BatteryState::from_u32(5), BatteryState::PendingCharge);
    assert!(BatteryState::PendingCharge.is_charging());
    assert_eq!(BatteryState::from_u32(6), BatteryState::PendingDischarge);
    // Invalid/unknown state values must map to Unknown
    assert_eq!(BatteryState::from_u32(7), BatteryState::Unknown);
    assert_eq!(BatteryState::from_u32(999), BatteryState::Unknown);
}

#[test]
fn test_daemon_status_clone_and_eq() {
    let status = DaemonStatus {
        version: "0.1.0".to_string(),
        uptime_seconds: 42,
        active_services: vec!["power".to_string(), "audio".to_string()],
    };

    let cloned = status.clone();
    assert_eq!(status, cloned);
    assert_eq!(cloned.version, "0.1.0");
    assert_eq!(cloned.uptime_seconds, 42);
    assert_eq!(cloned.active_services.len(), 2);
}

#[test]
fn test_service_health_states() {
    assert_eq!(ServiceHealth::Starting, ServiceHealth::Starting);
    assert_ne!(ServiceHealth::Healthy, ServiceHealth::Degraded);
    assert_ne!(ServiceHealth::Healthy, ServiceHealth::Stopped);
}
