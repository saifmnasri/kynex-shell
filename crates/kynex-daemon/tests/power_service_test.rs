//! Integration tests for `kynex-daemon` power service with a private `dbus-daemon`.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use kynex_daemon::services::power::{run_power_service, PowerServer};
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;
use zbus::object_server::SignalEmitter;

const UPOWER_SERVICE: &str = "org.freedesktop.UPower";
const UPOWER_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";

/// Helper managing a private, isolated `dbus-daemon` instance for integration testing.
struct TestBus {
    child: tokio::process::Child,
    address: String,
}

impl TestBus {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let mut cmd = tokio::process::Command::new("dbus-daemon");
        cmd.arg("--session")
            .arg("--print-address=1")
            .arg("--nofork")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture dbus-daemon stdout")?;
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();

        tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await??;
        let address = line.trim().to_string();

        Ok(Self { child, address })
    }

    fn address(&self) -> &str {
        &self.address
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Fake `UPower` device object implementing `org.freedesktop.UPower.Device`.
#[derive(Clone)]
struct FakeUPowerDevice {
    is_present: Arc<RwLock<bool>>,
    percentage: Arc<RwLock<f64>>,
    state: Arc<RwLock<u32>>,
    time_to_empty: Arc<RwLock<i64>>,
    time_to_full: Arc<RwLock<i64>>,
}

impl FakeUPowerDevice {
    fn new(
        present: bool,
        percentage: f64,
        state: u32,
        time_to_empty: i64,
        time_to_full: i64,
    ) -> Self {
        Self {
            is_present: Arc::new(RwLock::new(present)),
            percentage: Arc::new(RwLock::new(percentage)),
            state: Arc::new(RwLock::new(state)),
            time_to_empty: Arc::new(RwLock::new(time_to_empty)),
            time_to_full: Arc::new(RwLock::new(time_to_full)),
        }
    }
}

#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl FakeUPowerDevice {
    #[zbus(property)]
    async fn is_present(&self) -> bool {
        *self.is_present.read().await
    }

    #[zbus(property)]
    async fn percentage(&self) -> f64 {
        *self.percentage.read().await
    }

    #[zbus(property)]
    async fn state(&self) -> u32 {
        *self.state.read().await
    }

    #[zbus(property)]
    async fn time_to_empty(&self) -> i64 {
        *self.time_to_empty.read().await
    }

    #[zbus(property)]
    async fn time_to_full(&self) -> i64 {
        *self.time_to_full.read().await
    }
}

/// Spawns a fake `UPower` service on the private bus and returns connection and device handle.
async fn spawn_fake_upower(
    address: &str,
    device: FakeUPowerDevice,
) -> Result<(zbus::Connection, FakeUPowerDevice), Box<dyn Error>> {
    let conn = zbus::connection::Builder::address(address)
        .name(UPOWER_SERVICE)
        .serve_at(UPOWER_DEVICE_PATH, device.clone())
        .build()
        .await?;
    Ok((conn, device))
}

#[tokio::test]
async fn test_power_service_initial_values() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let fake_device = FakeUPowerDevice::new(true, 75.0, 2, 7200, 0);
    let (_fake_conn, _device) = spawn_fake_upower(bus.address(), fake_device).await?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let power_server = PowerServer::new();

    let srv = power_server.clone();
    let runner = tokio::spawn(async move {
        run_power_service(daemon_conn, srv, None, None).await;
    });

    // Wait for initial values to be populated
    let mut initial_ok = false;
    for _ in 0..50 {
        let snap = power_server.snapshot().await;
        if snap.present && (snap.percentage - 75.0).abs() < f64::EPSILON && snap.state == 2 {
            assert_eq!(snap.time_to_empty, 7200);
            assert_eq!(snap.time_to_full, 0);
            initial_ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(initial_ok, "Failed to capture initial UPower values");
    Ok(())
}

#[tokio::test]
async fn test_power_service_change_signal() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let fake_device = FakeUPowerDevice::new(true, 80.0, 2, 6000, 0);
    let (fake_conn, device) = spawn_fake_upower(bus.address(), fake_device).await?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let power_server = PowerServer::new();

    let srv = power_server.clone();
    let runner = tokio::spawn(async move {
        run_power_service(daemon_conn, srv, None, None).await;
    });

    // Wait for initial values
    let mut ready = false;
    for _ in 0..50 {
        let snap = power_server.snapshot().await;
        if snap.present && (snap.percentage - 80.0).abs() < f64::EPSILON {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        ready,
        "Power service failed to initialize with percentage 80.0"
    );

    // Emit change signals from fake UPower
    *device.percentage.write().await = 65.0;
    *device.time_to_empty.write().await = 4500;
    let emitter = SignalEmitter::new(&fake_conn, UPOWER_DEVICE_PATH)?;
    device.percentage_changed(&emitter).await?;
    device.time_to_empty_changed(&emitter).await?;

    // Verify power_server updated
    let mut updated = false;
    for _ in 0..50 {
        let snap = power_server.snapshot().await;
        if (snap.percentage - 65.0).abs() < f64::EPSILON && snap.time_to_empty == 4500 {
            updated = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(updated, "Power service failed to process change signal");
    Ok(())
}

#[tokio::test]
async fn test_power_service_upower_missing() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    // Do not spawn fake UPower service

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let power_server = PowerServer::new();

    let srv = power_server.clone();
    let runner = tokio::spawn(async move {
        run_power_service(daemon_conn, srv, None, None).await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let snap = power_server.snapshot().await;
    assert!(
        !snap.present,
        "Expected present=false when UPower is missing"
    );
    assert!((snap.percentage - 0.0).abs() < f64::EPSILON);
    assert_eq!(snap.state, 0);

    runner.abort();
    Ok(())
}

#[tokio::test]
async fn test_power_service_disappearing_and_reappearing() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let fake_device = FakeUPowerDevice::new(true, 50.0, 1, 0, 3600);
    let (fake_conn, _device) = spawn_fake_upower(bus.address(), fake_device).await?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let power_server = PowerServer::new();

    let srv = power_server.clone();
    let runner = tokio::spawn(async move {
        run_power_service(daemon_conn, srv, None, None).await;
    });

    // 1. Initial connection
    let mut stage1 = false;
    for _ in 0..50 {
        if power_server.snapshot().await.present {
            stage1 = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(stage1, "Initial connection failed");

    // 2. Disappear: drop fake UPower connection
    drop(fake_conn);

    let mut disappeared = false;
    for _ in 0..50 {
        if !power_server.snapshot().await.present {
            disappeared = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(disappeared, "Failed to detect UPower disappearance");

    // 3. Reappear: spawn new fake UPower on the bus
    let new_fake = FakeUPowerDevice::new(true, 92.0, 4, 0, 0);
    let (_new_conn, _new_dev) = spawn_fake_upower(bus.address(), new_fake).await?;

    let mut reappeared = false;
    for _ in 0..50 {
        let snap = power_server.snapshot().await;
        if snap.present && (snap.percentage - 92.0).abs() < f64::EPSILON {
            assert_eq!(snap.state, 4);
            reappeared = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(reappeared, "Failed to detect UPower reappearance");
    Ok(())
}

#[tokio::test]
async fn test_power_service_malformed_values() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    // Start with NaN percentage, state 99, negative time
    let fake_device = FakeUPowerDevice::new(true, f64::NAN, 99, -500, -100);
    let (fake_conn, device) = spawn_fake_upower(bus.address(), fake_device).await?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let power_server = PowerServer::new();

    let srv = power_server.clone();
    let runner = tokio::spawn(async move {
        run_power_service(daemon_conn, srv, None, None).await;
    });

    // Check sanitized initial values
    let mut sanitized1 = false;
    for _ in 0..50 {
        let snap = power_server.snapshot().await;
        if snap.present && (snap.percentage - 0.0).abs() < f64::EPSILON && snap.state == 0 {
            assert_eq!(snap.time_to_empty, 0);
            assert_eq!(snap.time_to_full, 0);
            sanitized1 = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        sanitized1,
        "Failed to sanitize NaN and out-of-range initial values"
    );

    // Send malformed update: percentage 250.0 (> 100.0)
    *device.percentage.write().await = 250.0;
    let emitter = SignalEmitter::new(&fake_conn, UPOWER_DEVICE_PATH)?;
    device.percentage_changed(&emitter).await?;

    let mut sanitized2 = false;
    for _ in 0..50 {
        let snap = power_server.snapshot().await;
        if (snap.percentage - 100.0).abs() < f64::EPSILON {
            sanitized2 = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(sanitized2, "Failed to clamp percentage > 100 to 100.0");
    Ok(())
}
