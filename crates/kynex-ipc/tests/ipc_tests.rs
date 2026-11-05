//! Tests for kynex-ipc interfaces, constants, and data structures.

use kynex_ipc::{
    interface_name, DaemonStatus, ServiceHealth, DAEMON_DESTINATION, DAEMON_INTERFACE, DAEMON_PATH,
    OBJECT_PATH_PREFIX,
};

#[test]
fn test_ipc_constants() {
    assert_eq!(DAEMON_DESTINATION, "org.kynexos.Daemon");
    assert_eq!(DAEMON_PATH, "/org/kynexos/Daemon");
    assert_eq!(DAEMON_INTERFACE, "org.kynexos.Daemon1");
    assert_eq!(OBJECT_PATH_PREFIX, "/org/kynexos");
    assert_eq!(interface_name("Daemon1"), "org.kynexos.Daemon1");
    assert_eq!(interface_name("Power1"), "org.kynexos.Power1");
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
