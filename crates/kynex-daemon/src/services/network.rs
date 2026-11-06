//! Network management service integrating with `NetworkManager`.

use futures_util::StreamExt;
use kynex_ipc::{NetworkKind, NetworkState};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::names::WellKnownName;
use zbus::object_server::SignalEmitter;
use zbus::proxy;
use zbus::{MatchRule, MessageStream, ObjectPath, OwnedObjectPath};

/// Returns `true` if the character is a Unicode bidi control or override character.
#[inline]
#[must_use]
pub fn is_bidi_override(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Sanitizes an SSID byte slice.
///
/// Rules:
/// 1. Capped at 32 bytes (maximum 802.11 SSID length).
/// 2. Converted safely to UTF-8 without panicking (lossy conversion).
/// 3. Drops all control characters (`c.is_control()`, e.g. NUL, tabs, newlines, C0/C1).
/// 4. Drops Unicode bidi formatting and override characters:
///    - U+202A..=U+202E (LRE, RLE, PDF, LRO, RLO)
///    - U+2066..=U+2069 (LRI, RLI, FSI, PDI)
/// 5. Never panics under any input.
#[must_use]
pub fn sanitize_ssid(raw: &[u8]) -> String {
    let capped = if raw.len() > 32 { &raw[..32] } else { raw };

    let lossy = String::from_utf8_lossy(capped);
    let mut out = String::with_capacity(lossy.len());

    for c in lossy.chars() {
        if c.is_control() || is_bidi_override(c) || c == '\u{FFFD}' {
            continue;
        }
        out.push(c);
    }

    out
}

/// Sanitizes signal strength percentage, clamping to 0..=100.
#[inline]
#[must_use]
pub fn sanitize_signal_strength(val: u32) -> u32 {
    if val > 100 {
        100
    } else {
        val
    }
}

/// Maps `NetworkManager` `NMState` enum to `NetworkState`.
#[must_use]
pub fn map_nm_state(nm_state: u32) -> NetworkState {
    match nm_state {
        10 | 20 | 30 => NetworkState::Disconnected,
        40 => NetworkState::Connecting,
        50 | 60 | 70 => NetworkState::Connected,
        _ => NetworkState::Unknown,
    }
}

/// Maps active connection type string to `NetworkKind`.
#[must_use]
pub fn map_connection_type(conn_type: &str) -> NetworkKind {
    match conn_type {
        "802-11-wireless" | "wifi" => NetworkKind::Wifi,
        "802-3-ethernet" | "ethernet" => NetworkKind::Ethernet,
        "" => NetworkKind::None,
        _ => NetworkKind::Other,
    }
}

/// Internal state snapshot for network connectivity metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSnapshot {
    /// Whether network hardware/service is managed and present.
    pub present: bool,
    /// Connection state (0=Disconnected, 1=Connecting, 2=Connected, 3=Unknown).
    pub state: u32,
    /// Connection medium kind (0=None, 1=Wifi, 2=Ethernet, 3=Other).
    pub kind: u32,
    /// Wi-Fi signal strength (0 to 100).
    pub signal_strength: u32,
    /// Sanitized SSID string for active Wi-Fi connection.
    pub ssid: String,
}

impl Default for NetworkSnapshot {
    fn default() -> Self {
        Self {
            present: false,
            state: NetworkState::Disconnected as u32,
            kind: NetworkKind::None as u32,
            signal_strength: 0,
            ssid: String::new(),
        }
    }
}

/// `NetworkManager` root proxy.
#[proxy(
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager",
    interface = "org.freedesktop.NetworkManager"
)]
pub trait NetworkManager {
    /// Operational state of the `NetworkManager` daemon.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// The object path of the primary active connection.
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;

    /// List of realized network device object paths.
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

/// `ActiveConnection` proxy.
#[proxy(
    default_service = "org.freedesktop.NetworkManager",
    interface = "org.freedesktop.NetworkManager.Connection.Active"
)]
pub trait ActiveConnection {
    /// The connection type (e.g. 802-11-wireless, 802-3-ethernet).
    #[zbus(property)]
    fn type_(&self) -> zbus::Result<String>;

    /// Active connection state.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// The path of the specific object (e.g. `AccessPoint` for wireless).
    #[zbus(property)]
    fn specific_object(&self) -> zbus::Result<OwnedObjectPath>;
}

/// `AccessPoint` proxy for wireless networks.
#[proxy(
    default_service = "org.freedesktop.NetworkManager",
    interface = "org.freedesktop.NetworkManager.AccessPoint"
)]
pub trait AccessPoint {
    /// Signal strength percentage (0-100).
    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;

    /// Raw SSID byte array.
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;
}

/// D-Bus object implementing `org.kynexos.Daemon.Network1`.
#[derive(Debug, Clone, Default)]
pub struct NetworkServer {
    state: Arc<RwLock<NetworkSnapshot>>,
}

impl NetworkServer {
    /// Creates a new `NetworkServer` instance with default values (present=false).
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(NetworkSnapshot::default())),
        }
    }

    /// Returns a copy of the current `NetworkSnapshot`.
    pub async fn snapshot(&self) -> NetworkSnapshot {
        self.state.read().await.clone()
    }

    /// Updates internal state and emits D-Bus `PropertiesChanged` signals if changed.
    pub async fn update(&self, new_state: NetworkSnapshot, emitter: Option<&SignalEmitter<'_>>) {
        let (old, changed) = {
            let mut guard = self.state.write().await;
            let old = guard.clone();
            if old == new_state {
                (old, false)
            } else {
                *guard = new_state.clone();
                (old, true)
            }
        };

        if !changed {
            return;
        }

        if let Some(em) = emitter {
            if old.present != new_state.present {
                let _ = self.present_changed(em).await;
            }
            if old.state != new_state.state {
                let _ = self.state_changed(em).await;
            }
            if old.kind != new_state.kind {
                let _ = self.kind_changed(em).await;
            }
            if old.signal_strength != new_state.signal_strength {
                let _ = self.signal_strength_changed(em).await;
            }
            if old.ssid != new_state.ssid {
                let _ = self.ssid_changed(em).await;
            }
        }
    }
}

#[zbus::interface(name = "org.kynexos.Daemon.Network1")]
impl NetworkServer {
    /// Whether network hardware/connectivity is managed and present.
    #[zbus(property)]
    async fn present(&self) -> bool {
        self.state.read().await.present
    }

    /// Connection state (0=Disconnected, 1=Connecting, 2=Connected, 3=Unknown).
    #[zbus(property)]
    async fn state(&self) -> u32 {
        self.state.read().await.state
    }

    /// Connection medium kind (0=None, 1=Wifi, 2=Ethernet, 3=Other).
    #[zbus(property)]
    async fn kind(&self) -> u32 {
        self.state.read().await.kind
    }

    /// Wi-Fi signal strength clamped between 0 and 100.
    #[zbus(property)]
    async fn signal_strength(&self) -> u32 {
        self.state.read().await.signal_strength
    }

    /// Sanitized SSID string for active Wi-Fi connection.
    #[zbus(property)]
    async fn ssid(&self) -> String {
        self.state.read().await.ssid.clone()
    }
}

/// Checks whether the target D-Bus service is currently registered on the bus.
async fn check_service_owner(dbus_proxy: &DBusProxy<'_>, service_name: &str) -> bool {
    let wk_name: Result<WellKnownName<'_>, _> = service_name.try_into();
    match wk_name {
        Ok(ref name) => dbus_proxy
            .get_name_owner(zbus::names::BusName::WellKnown(name.as_ref()))
            .await
            .is_ok(),
        Err(_) => false,
    }
}

/// Reads the current network snapshot from `NetworkManager` over the connection.
pub async fn read_network_snapshot(conn: &zbus::Connection, service_name: &str) -> NetworkSnapshot {
    let nm_proxy = match NetworkManagerProxy::builder(conn)
        .destination(service_name)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
    {
        Ok(p) => p,
        Err(err) => {
            warn!("Failed to create NetworkManagerProxy: {err}");
            return NetworkSnapshot::default();
        }
    };

    // Check devices: if no devices present, mark present = false
    let devices = nm_proxy.devices().await.unwrap_or_default();
    if devices.is_empty() {
        debug!("NetworkManager reports 0 devices; marking present=false");
        return NetworkSnapshot {
            present: false,
            ..Default::default()
        };
    }

    let raw_state = nm_proxy.state().await.unwrap_or(0);
    let state = map_nm_state(raw_state);

    let default_path: OwnedObjectPath = match ObjectPath::try_from("/") {
        Ok(p) => p.into(),
        Err(_) => return NetworkSnapshot::default(),
    };

    let primary_path = nm_proxy
        .primary_connection()
        .await
        .unwrap_or_else(|_| default_path.clone());

    let primary_str = primary_path.as_str();
    if primary_str == "/" || primary_str.is_empty() {
        return NetworkSnapshot {
            present: true,
            state: state as u32,
            kind: NetworkKind::None as u32,
            signal_strength: 0,
            ssid: String::new(),
        };
    }

    // Read active connection
    let active_proxy = match ActiveConnectionProxy::builder(conn)
        .destination(service_name)
        .path(primary_path)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
    {
        Ok(p) => p,
        Err(err) => {
            warn!("Failed to create ActiveConnectionProxy: {err}");
            return NetworkSnapshot {
                present: true,
                state: state as u32,
                kind: NetworkKind::None as u32,
                signal_strength: 0,
                ssid: String::new(),
            };
        }
    };

    let conn_type = active_proxy.type_().await.unwrap_or_default();
    let kind = map_connection_type(&conn_type);

    let (signal_strength, ssid) = if kind == NetworkKind::Wifi {
        let specific_path = active_proxy.specific_object().await.unwrap_or(default_path);

        if specific_path.as_str() != "/" && !specific_path.as_str().is_empty() {
            match AccessPointProxy::builder(conn)
                .destination(service_name)
                .path(specific_path)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
            {
                Ok(ap) => {
                    let raw_strength = ap.strength().await.unwrap_or(0);
                    let raw_ssid = ap.ssid().await.unwrap_or_default();
                    (
                        sanitize_signal_strength(u32::from(raw_strength)),
                        sanitize_ssid(&raw_ssid),
                    )
                }
                Err(err) => {
                    warn!("Failed to create AccessPointProxy: {err}");
                    (0, String::new())
                }
            }
        } else {
            (0, String::new())
        }
    } else {
        (0, String::new())
    };

    NetworkSnapshot {
        present: true,
        state: state as u32,
        kind: kind as u32,
        signal_strength,
        ssid,
    }
}

/// Watches `NetworkManager` events and updates the `NetworkServer`.
///
/// Signal-driven: connects via D-Bus signals, never polls. If `NetworkManager` or network
/// devices are missing, sets `present = false` without panicking. Reconnects automatically.
pub async fn run_network_service(
    nm_conn: zbus::Connection,
    server: NetworkServer,
    emitter: Option<SignalEmitter<'static>>,
    nm_service: Option<String>,
    ready_tx: Option<tokio::sync::oneshot::Sender<()>>,
) {
    let service_name = nm_service.unwrap_or_else(|| "org.freedesktop.NetworkManager".to_string());

    let dbus_proxy = match DBusProxy::new(&nm_conn).await {
        Ok(proxy) => proxy,
        Err(err) => {
            warn!("Failed to create DBusProxy on NetworkManager bus: {err}");
            if let Some(tx) = ready_tx {
                let _ = tx.send(());
            }
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, service_name.as_str())])
        .await
        .ok();

    // Subscribe to signals in the /org/freedesktop/NetworkManager path namespace
    let nm_rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path_namespace("/org/freedesktop/NetworkManager")
        .build()
        .ok();

    let mut nm_stream = if let Some(rule) = nm_rule {
        MessageStream::for_match_rule(rule, &nm_conn, None)
            .await
            .ok()
    } else {
        None
    };

    let is_running = check_service_owner(&dbus_proxy, &service_name).await;

    if is_running {
        let snapshot = read_network_snapshot(&nm_conn, &service_name).await;
        server.update(snapshot, emitter.as_ref()).await;
    } else {
        server
            .update(NetworkSnapshot::default(), emitter.as_ref())
            .await;
    }

    // Notify ready_tx if provided (for integration tests)
    if let Some(tx) = ready_tx {
        let _ = tx.send(());
    }

    info!("NetworkManager event listener active (signal-driven, 0 polling)");

    loop {
        tokio::select! {
            Some(name_event) = async {
                if let Some(ref mut s) = name_owner_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = name_event.args() {
                    let new_owner = args.new_owner();
                    if new_owner.as_ref().is_none_or(|o| o.as_str().is_empty()) {
                        info!("NetworkManager service left the bus; resetting network state");
                        server.update(NetworkSnapshot::default(), emitter.as_ref()).await;
                    } else {
                        info!("NetworkManager service appeared on the bus; fetching snapshot");
                        let snapshot = read_network_snapshot(&nm_conn, &service_name).await;
                        server.update(snapshot, emitter.as_ref()).await;
                    }
                }
            }
            Some(Ok(_signal)) = async {
                if let Some(ref mut s) = nm_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                debug!("NetworkManager change signal received; refreshing snapshot");
                let snapshot = read_network_snapshot(&nm_conn, &service_name).await;
                server.update(snapshot, emitter.as_ref()).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_ssid_valid() {
        assert_eq!(sanitize_ssid(b"MyHomeWiFi"), "MyHomeWiFi");
        assert_eq!(sanitize_ssid(b"Office-Network_5G"), "Office-Network_5G");
    }

    #[test]
    fn test_sanitize_ssid_cap_32_bytes() {
        let long_ssid = b"12345678901234567890123456789012345EXTRA";
        assert_eq!(long_ssid.len(), 40);
        let sanitized = sanitize_ssid(long_ssid);
        assert_eq!(sanitized, "12345678901234567890123456789012");
        assert_eq!(sanitized.len(), 32);
    }

    #[test]
    fn test_sanitize_ssid_control_characters() {
        // NUL, newline, tab, carriage return
        assert_eq!(sanitize_ssid(b"Wifi\0With\x00Nul"), "WifiWithNul");
        assert_eq!(sanitize_ssid(b"Line\nBreak\r\tTest"), "LineBreakTest");
    }

    #[test]
    fn test_sanitize_ssid_bidi_and_overrides() {
        // U+202E (RLO), U+202A (LRE), U+2066 (LRI), U+2069 (PDI)
        let evil = "Evil\u{202E}Network\u{202A}\u{2066}Test\u{2069}";
        let sanitized = sanitize_ssid(evil.as_bytes());
        assert_eq!(sanitized, "EvilNetworkTest");
    }

    #[test]
    fn test_sanitize_ssid_invalid_utf8() {
        let invalid = b"Bad\xFF\xFE\x80Bytes";
        let sanitized = sanitize_ssid(invalid);
        assert_eq!(sanitized, "BadBytes");
    }

    #[test]
    fn test_sanitize_ssid_emoji() {
        let emoji_ssid = "🚀 KynexNet 🌐";
        let sanitized = sanitize_ssid(emoji_ssid.as_bytes());
        assert_eq!(sanitized, "🚀 KynexNet 🌐");
    }

    #[test]
    fn test_sanitize_ssid_empty() {
        assert_eq!(sanitize_ssid(b""), "");
    }

    #[test]
    fn test_sanitize_signal_strength() {
        assert_eq!(sanitize_signal_strength(0), 0);
        assert_eq!(sanitize_signal_strength(50), 50);
        assert_eq!(sanitize_signal_strength(100), 100);
        assert_eq!(sanitize_signal_strength(101), 100);
        assert_eq!(sanitize_signal_strength(9999), 100);
    }

    #[test]
    fn test_map_nm_state() {
        assert_eq!(map_nm_state(0), NetworkState::Unknown);
        assert_eq!(map_nm_state(10), NetworkState::Disconnected);
        assert_eq!(map_nm_state(20), NetworkState::Disconnected);
        assert_eq!(map_nm_state(30), NetworkState::Disconnected);
        assert_eq!(map_nm_state(40), NetworkState::Connecting);
        assert_eq!(map_nm_state(50), NetworkState::Connected);
        assert_eq!(map_nm_state(60), NetworkState::Connected);
        assert_eq!(map_nm_state(70), NetworkState::Connected);
        assert_eq!(map_nm_state(99), NetworkState::Unknown);
    }

    #[test]
    fn test_map_connection_type() {
        assert_eq!(map_connection_type("802-11-wireless"), NetworkKind::Wifi);
        assert_eq!(map_connection_type("wifi"), NetworkKind::Wifi);
        assert_eq!(map_connection_type("802-3-ethernet"), NetworkKind::Ethernet);
        assert_eq!(map_connection_type("ethernet"), NetworkKind::Ethernet);
        assert_eq!(map_connection_type(""), NetworkKind::None);
        assert_eq!(map_connection_type("vpn"), NetworkKind::Other);
        assert_eq!(map_connection_type("gsm"), NetworkKind::Other);
    }

    #[tokio::test]
    async fn test_network_server_direct_getters_and_update() {
        let server = NetworkServer::new();
        assert!(!server.present().await);
        assert_eq!(server.state().await, NetworkState::Disconnected as u32);
        assert_eq!(server.kind().await, NetworkKind::None as u32);
        assert_eq!(server.signal_strength().await, 0);
        assert_eq!(server.ssid().await, "");

        let new_snapshot = NetworkSnapshot {
            present: true,
            state: NetworkState::Connected as u32,
            kind: NetworkKind::Wifi as u32,
            signal_strength: 85,
            ssid: "TestOfficeWifi".to_string(),
        };

        server.update(new_snapshot.clone(), None).await;
        assert!(server.present().await);
        assert_eq!(server.state().await, NetworkState::Connected as u32);
        assert_eq!(server.kind().await, NetworkKind::Wifi as u32);
        assert_eq!(server.signal_strength().await, 85);
        assert_eq!(server.ssid().await, "TestOfficeWifi");
        assert_eq!(server.snapshot().await, new_snapshot);

        // Update with identical state (no-op branch)
        server.update(new_snapshot, None).await;
    }
}
