//! Integration tests for `kynex-daemon` network service with a private `dbus-daemon`.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use kynex_daemon::services::network::{run_network_service, NetworkServer};
use kynex_ipc::{NetworkKind, NetworkState};
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;
use zbus::object_server::SignalEmitter;
use zbus::OwnedObjectPath;

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_CONN_PATH: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
const NM_AP_PATH: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";

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

/// Fake `NetworkManager` object.
#[derive(Clone)]
struct FakeNM {
    state: Arc<RwLock<u32>>,
    primary_connection: Arc<RwLock<OwnedObjectPath>>,
    devices: Arc<RwLock<Vec<OwnedObjectPath>>>,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl FakeNM {
    #[zbus(property)]
    async fn state(&self) -> u32 {
        *self.state.read().await
    }

    #[zbus(property)]
    async fn primary_connection(&self) -> OwnedObjectPath {
        self.primary_connection.read().await.clone()
    }

    #[zbus(property)]
    async fn devices(&self) -> Vec<OwnedObjectPath> {
        self.devices.read().await.clone()
    }
}

/// Fake `ActiveConnection` object.
#[derive(Clone)]
struct FakeActiveConnection {
    conn_type: Arc<RwLock<String>>,
    state: Arc<RwLock<u32>>,
    specific_object: Arc<RwLock<OwnedObjectPath>>,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl FakeActiveConnection {
    #[zbus(property)]
    async fn type_(&self) -> String {
        self.conn_type.read().await.clone()
    }

    #[zbus(property)]
    async fn state(&self) -> u32 {
        *self.state.read().await
    }

    #[zbus(property)]
    async fn specific_object(&self) -> OwnedObjectPath {
        self.specific_object.read().await.clone()
    }
}

/// Fake `AccessPoint` object.
#[derive(Clone)]
struct FakeAccessPoint {
    strength: Arc<RwLock<u8>>,
    ssid: Arc<RwLock<Vec<u8>>>,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl FakeAccessPoint {
    #[zbus(property)]
    async fn strength(&self) -> u8 {
        *self.strength.read().await
    }

    #[zbus(property)]
    async fn ssid(&self) -> Vec<u8> {
        self.ssid.read().await.clone()
    }
}

#[allow(dead_code)]
struct FakeNetworkStack {
    conn: zbus::Connection,
    nm: FakeNM,
    active_conn: FakeActiveConnection,
    ap: FakeAccessPoint,
}

async fn spawn_fake_nm_stack(
    address: &str,
    state: u32,
    has_device: bool,
    conn_type: &str,
    strength: u8,
    ssid: &[u8],
) -> Result<FakeNetworkStack, Box<dyn Error>> {
    let dev_path = OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Devices/1")?;
    let prim_path = OwnedObjectPath::try_from(NM_CONN_PATH)?;
    let ap_path = OwnedObjectPath::try_from(NM_AP_PATH)?;

    let devices = if has_device { vec![dev_path] } else { vec![] };

    let fake_nm = FakeNM {
        state: Arc::new(RwLock::new(state)),
        primary_connection: Arc::new(RwLock::new(prim_path.clone())),
        devices: Arc::new(RwLock::new(devices)),
    };

    let fake_active = FakeActiveConnection {
        conn_type: Arc::new(RwLock::new(conn_type.to_string())),
        state: Arc::new(RwLock::new(2)), // Activated
        specific_object: Arc::new(RwLock::new(ap_path)),
    };

    let fake_ap = FakeAccessPoint {
        strength: Arc::new(RwLock::new(strength)),
        ssid: Arc::new(RwLock::new(ssid.to_vec())),
    };

    let conn = zbus::connection::Builder::address(address)
        .name(NM_SERVICE)
        .serve_at(NM_PATH, fake_nm.clone())
        .serve_at(NM_CONN_PATH, fake_active.clone())
        .serve_at(NM_AP_PATH, fake_ap.clone())
        .build()
        .await?;

    Ok(FakeNetworkStack {
        conn,
        nm: fake_nm,
        active_conn: fake_active,
        ap: fake_ap,
    })
}

#[tokio::test]
async fn test_network_service_initial_values() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let _stack =
        spawn_fake_nm_stack(bus.address(), 70, true, "802-11-wireless", 75, b"HomeWiFi").await?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let server = NetworkServer::new();
    let srv = server.clone();

    let runner = tokio::spawn(async move {
        run_network_service(daemon_conn, srv, None, None, None).await;
    });

    let mut ready = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if snap.present
            && snap.state == NetworkState::Connected as u32
            && snap.kind == NetworkKind::Wifi as u32
            && snap.signal_strength == 75
            && snap.ssid == "HomeWiFi"
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(
        ready,
        "Network service failed to capture initial Wi-Fi snapshot"
    );
    Ok(())
}

#[tokio::test]
async fn test_network_service_change_signal() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let stack = spawn_fake_nm_stack(
        bus.address(),
        70,
        true,
        "802-11-wireless",
        80,
        b"InitialNet",
    )
    .await?;

    let server = NetworkServer::new();
    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .serve_at(kynex_ipc::NETWORK_PATH, server.clone())
        .build()
        .await?;
    let emitter = SignalEmitter::new(&daemon_conn, kynex_ipc::NETWORK_PATH).ok();

    let srv = server.clone();
    let runner = tokio::spawn(async move {
        run_network_service(daemon_conn, srv, emitter, None, None).await;
    });

    // Wait for initial values
    let mut initial_ok = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if snap.present && snap.signal_strength == 80 && snap.ssid == "InitialNet" {
            initial_ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(initial_ok, "Initial values not captured");

    // Update fake access point and emit change signals
    *stack.ap.strength.write().await = 45;
    *stack.ap.ssid.write().await = b"UpdatedCoffeeNet".to_vec();

    let ap_emitter = SignalEmitter::new(&stack.conn, NM_AP_PATH)?;
    stack.ap.strength_changed(&ap_emitter).await?;
    stack.ap.ssid_changed(&ap_emitter).await?;

    let mut updated = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if snap.signal_strength == 45 && snap.ssid == "UpdatedCoffeeNet" {
            updated = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(updated, "Network service failed to process change signal");
    Ok(())
}

#[tokio::test]
async fn test_network_service_nm_missing() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let server = NetworkServer::new();
    let srv = server.clone();

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let runner = tokio::spawn(async move {
        run_network_service(daemon_conn, srv, None, None, Some(ready_tx)).await;
    });

    ready_rx.await?;
    let snap = server.snapshot().await;
    assert!(
        !snap.present,
        "Service should report present=false when NM is missing"
    );
    assert_eq!(snap.state, NetworkState::Disconnected as u32);
    assert_eq!(snap.kind, NetworkKind::None as u32);
    assert_eq!(snap.signal_strength, 0);
    assert_eq!(snap.ssid, "");

    runner.abort();
    Ok(())
}

#[tokio::test]
async fn test_network_service_disappearing_and_reappearing() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let stack = spawn_fake_nm_stack(bus.address(), 70, true, "802-3-ethernet", 0, b"").await?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let server = NetworkServer::new();
    let srv = server.clone();

    let runner = tokio::spawn(async move {
        run_network_service(daemon_conn, srv, None, None, None).await;
    });

    // Verify initial ethernet state
    let mut ready = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if snap.present && snap.kind == NetworkKind::Ethernet as u32 {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready, "Initial ethernet connection not captured");

    // NetworkManager disappears (close fake connection)
    drop(stack);

    let mut disappeared = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if !snap.present {
            disappeared = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        disappeared,
        "Service should mark present=false when NM drops off bus"
    );

    // Re-spawn NetworkManager with Wi-Fi
    let _new_stack = spawn_fake_nm_stack(
        bus.address(),
        70,
        true,
        "802-11-wireless",
        90,
        b"ReconnectedWifi",
    )
    .await?;

    let mut restored = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if snap.present
            && snap.kind == NetworkKind::Wifi as u32
            && snap.signal_strength == 90
            && snap.ssid == "ReconnectedWifi"
        {
            restored = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(restored, "Service failed to restore after NM re-appeared");
    Ok(())
}

#[tokio::test]
async fn test_network_service_malformed_values() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    // Out-of-bounds state (999), strength > 100 (240), unknown connection type
    let _stack =
        spawn_fake_nm_stack(bus.address(), 999, true, "bluetooth-pan", 240, b"BTNet").await?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let server = NetworkServer::new();
    let srv = server.clone();

    let runner = tokio::spawn(async move {
        run_network_service(daemon_conn, srv, None, None, None).await;
    });

    let mut validated = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if snap.present
            && snap.state == NetworkState::Unknown as u32
            && snap.kind == NetworkKind::Other as u32
        {
            validated = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(validated, "Malformed values not safely handled");
    Ok(())
}

#[tokio::test]
async fn test_network_service_no_devices() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    // NM running, but devices list is empty
    let _stack =
        spawn_fake_nm_stack(bus.address(), 70, false, "802-11-wireless", 80, b"Wifi").await?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let server = NetworkServer::new();
    let srv = server.clone();

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let runner = tokio::spawn(async move {
        run_network_service(daemon_conn, srv, None, None, Some(ready_tx)).await;
    });

    ready_rx.await?;
    let snap = server.snapshot().await;
    assert!(
        !snap.present,
        "When NM has 0 devices, present should be false"
    );

    runner.abort();
    Ok(())
}

#[tokio::test]
async fn test_network_service_no_primary_connection() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let stack = spawn_fake_nm_stack(bus.address(), 20, true, "", 0, b"").await?;
    // Set primary connection to "/"
    *stack.nm.primary_connection.write().await = OwnedObjectPath::try_from("/")?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let server = NetworkServer::new();
    let srv = server.clone();

    let runner = tokio::spawn(async move {
        run_network_service(daemon_conn, srv, None, None, None).await;
    });

    let mut captured = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if snap.present
            && snap.state == NetworkState::Disconnected as u32
            && snap.kind == NetworkKind::None as u32
        {
            captured = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(
        captured,
        "Failed to capture disconnected state without primary connection"
    );
    Ok(())
}

#[tokio::test]
async fn test_network_service_wifi_no_access_point() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;
    let stack = spawn_fake_nm_stack(bus.address(), 70, true, "802-11-wireless", 0, b"").await?;
    // Set specific object to "/" (no AP)
    *stack.active_conn.specific_object.write().await = OwnedObjectPath::try_from("/")?;

    let daemon_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let server = NetworkServer::new();
    let srv = server.clone();

    let runner = tokio::spawn(async move {
        run_network_service(daemon_conn, srv, None, None, None).await;
    });

    let mut captured = false;
    for _ in 0..50 {
        let snap = server.snapshot().await;
        if snap.present
            && snap.kind == NetworkKind::Wifi as u32
            && snap.signal_strength == 0
            && snap.ssid.is_empty()
        {
            captured = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    runner.abort();
    assert!(
        captured,
        "Failed to handle Wi-Fi connection without AP object"
    );
    Ok(())
}

#[tokio::test]
async fn test_network_service_adversarial_ssids() -> Result<(), Box<dyn Error>> {
    let bus = TestBus::new().await?;

    let cases: Vec<(&[u8], &str)> = vec![
        // 1. Bidi override: U+202E (RLO) and U+2066 (LRI)
        (
            "Adversarial\u{202E}Bidi\u{2066}Net".as_bytes(),
            "AdversarialBidiNet",
        ),
        // 2. NUL bytes
        (b"Null\x00In\x00SSID", "NullInSSID"),
        // 3. 33+ bytes (length capped to 32)
        (
            b"1234567890123456789012345678901234567890",
            "12345678901234567890123456789012",
        ),
        // 4. Invalid UTF-8 bytes
        (b"Corrupt\xff\xfe\x80Bytes", "CorruptBytes"),
        // 5. Valid Unicode Emoji
        ("🔥 FastNet 🚀".as_bytes(), "🔥 FastNet 🚀"),
        // 6. Empty SSID
        (b"", ""),
        // 7. Control characters (newlines, tabs)
        (b"Line\nTab\tCR\rName", "LineTabCRName"),
    ];

    for (raw_ssid, expected_clean) in cases {
        let stack =
            spawn_fake_nm_stack(bus.address(), 70, true, "802-11-wireless", 80, raw_ssid).await?;
        let daemon_conn = zbus::connection::Builder::address(bus.address())
            .build()
            .await?;
        let server = NetworkServer::new();
        let srv = server.clone();

        let runner = tokio::spawn(async move {
            run_network_service(daemon_conn, srv, None, None, None).await;
        });

        let mut matched = false;
        for _ in 0..50 {
            let snap = server.snapshot().await;
            if snap.present && snap.ssid == expected_clean {
                matched = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        runner.abort();
        drop(stack);
        assert!(
            matched,
            "Adversarial SSID {:?} expected {:?}, got mismatch",
            String::from_utf8_lossy(raw_ssid),
            expected_clean
        );
    }

    Ok(())
}
