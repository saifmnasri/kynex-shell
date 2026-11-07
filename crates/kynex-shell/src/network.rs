//! Network management client model exposing connectivity state to QML via `cxx-qt`.

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use futures_util::StreamExt;
use kynex_ipc::network::{Network1Proxy, NetworkKind, NetworkState, NETWORK_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use tracing::{debug, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::names::WellKnownName;
use zbus::{MatchRule, MessageStream};

/// Pure logic representation of the network connectivity status.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkClientState {
    /// Whether network hardware/service is managed and present.
    pub present: bool,
    /// Whether the network is currently connected and active.
    pub connected: bool,
    /// Connection medium kind (0=None, 1=Wifi, 2=Ethernet, 3=Other).
    pub kind: i32,
    /// Whether connection is Wi-Fi.
    pub wifi: bool,
    /// Whether connection is wired Ethernet.
    pub ethernet: bool,
    /// Signal strength percentage (0-100).
    pub strength: i32,
    /// Sanitized SSID string for active Wi-Fi connection.
    pub ssid: String,
    /// Whether Wi-Fi signal is weak (below threshold).
    pub weak: bool,
}

impl NetworkClientState {
    /// Constructs a validated `NetworkClientState` from raw D-Bus values.
    #[must_use]
    pub fn from_raw(
        present: bool,
        raw_state: u32,
        raw_kind: u32,
        raw_strength: u32,
        raw_ssid: &str,
    ) -> Self {
        if !present {
            return Self::default();
        }

        let state = NetworkState::from_u32(raw_state);
        let kind = NetworkKind::from_u32(raw_kind);
        let connected = state == NetworkState::Connected;
        let clamped_strength = if raw_strength > 100 {
            100
        } else {
            raw_strength
        };
        let strength = i32::try_from(clamped_strength).unwrap_or(0);

        let wifi = kind == NetworkKind::Wifi;
        let ethernet = kind == NetworkKind::Ethernet;
        let weak =
            connected && wifi && clamped_strength < kynex_common::NETWORK_WEAK_SIGNAL_PERCENTAGE;

        Self {
            present: true,
            connected,
            kind: kind as i32,
            wifi,
            ethernet,
            strength,
            ssid: raw_ssid.to_string(),
            weak,
        }
    }
}

/// CXX-Qt bridge module for the `NetworkModel` `QObject`.
#[cxx_qt::bridge]
pub mod qobject {
    // SAFETY: FFI bridge declarations to Qt's QString provided by cxx-qt-lib.
    // QString is a standard Qt value type whose C++ layout and invariants are managed
    // by cxx-qt-lib and guaranteed memory-safe across the FFI boundary.
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Alias to Qt QString
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Network status model exposed to QML.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, present)]
        #[qproperty(bool, connected)]
        #[qproperty(i32, kind)]
        #[qproperty(bool, wifi)]
        #[qproperty(bool, ethernet)]
        #[qproperty(i32, strength)]
        #[qproperty(QString, ssid)]
        #[qproperty(bool, weak)]
        type NetworkModel = super::NetworkModelRust;

        /// Starts the background D-Bus listener connecting to `org.kynexos.Daemon`.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for NetworkModel {}
}

/// Backing data for the `NetworkModel` `QObject`.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default)]
pub struct NetworkModelRust {
    present: bool,
    connected: bool,
    kind: i32,
    wifi: bool,
    ethernet: bool,
    strength: i32,
    ssid: QString,
    weak: bool,
}

impl qobject::NetworkModel {
    /// Initialize the background D-Bus client connecting to `kynex-daemon`.
    pub fn init(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_network_listener(qt_thread).await;
        });
    }
}

/// Reads the current snapshot from `Network1Proxy`, converting to `NetworkClientState`.
async fn fetch_network_state(proxy: &Network1Proxy<'_>) -> NetworkClientState {
    let present = proxy.present().await.unwrap_or(false);
    let state = proxy.state().await.unwrap_or(0);
    let kind = proxy.kind().await.unwrap_or(0);
    let strength = proxy.signal_strength().await.unwrap_or(0);
    let ssid = proxy.ssid().await.unwrap_or_default();
    NetworkClientState::from_raw(present, state, kind, strength, &ssid)
}

/// Dispatches a `NetworkClientState` update to the Qt main thread via `CxxQtThread`.
fn dispatch_state(
    qt_thread: &cxx_qt::CxxQtThread<qobject::NetworkModel>,
    state: NetworkClientState,
) {
    let _ = qt_thread.queue(move |mut qobj| {
        qobj.as_mut().set_present(state.present);
        qobj.as_mut().set_connected(state.connected);
        qobj.as_mut().set_kind(state.kind);
        qobj.as_mut().set_wifi(state.wifi);
        qobj.as_mut().set_ethernet(state.ethernet);
        qobj.as_mut().set_strength(state.strength);
        qobj.as_mut().set_ssid(QString::from(&state.ssid));
        qobj.as_mut().set_weak(state.weak);
    });
}

/// Attempts to connect to `org.kynexos.Daemon` if present on the bus.
async fn create_daemon_proxy<'a>(
    conn: &'a zbus::Connection,
    dbus_proxy: &DBusProxy<'_>,
) -> Option<Network1Proxy<'a>> {
    let wk_name: Result<WellKnownName<'_>, _> = DAEMON_DESTINATION.try_into();
    let has_daemon = match wk_name {
        Ok(ref name) => dbus_proxy
            .get_name_owner(zbus::names::BusName::WellKnown(name.as_ref()))
            .await
            .is_ok(),
        Err(_) => false,
    };

    if has_daemon {
        Network1Proxy::builder(conn)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .ok()
    } else {
        None
    }
}

/// Initializes a match-rule signal stream on `NETWORK_PATH`.
async fn init_signal_stream(conn: &zbus::Connection) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path(NETWORK_PATH)
        .build()
        .ok()?;
    MessageStream::for_match_rule(rule, conn, None).await.ok()
}

/// Background asynchronous listener connecting to `org.kynexos.Daemon.Network1`.
///
/// Signal-driven: listens for property updates and reconnects automatically
/// when `kynex-daemon` restarts without blocking the Qt GUI thread.
pub async fn run_network_listener(qt_thread: cxx_qt::CxxQtThread<qobject::NetworkModel>) {
    let session_conn = match zbus::connection::Builder::session().build().await {
        Ok(c) => c,
        Err(err) => {
            warn!("NetworkModel failed to connect to session bus: {err}");
            dispatch_state(&qt_thread, NetworkClientState::default());
            return;
        }
    };

    let dbus_proxy = match DBusProxy::new(&session_conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("NetworkModel failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
        .ok();

    let mut signal_stream = init_signal_stream(&session_conn).await;
    let mut network_proxy = create_daemon_proxy(&session_conn, &dbus_proxy).await;

    if let Some(ref proxy) = network_proxy {
        let state = fetch_network_state(proxy).await;
        info!(
            "NetworkModel connected to daemon: present={}, connected={}, kind={}, strength={}%",
            state.present, state.connected, state.kind, state.strength
        );
        dispatch_state(&qt_thread, state);
    } else {
        info!("kynex-daemon not found on bus; setting present=false");
        dispatch_state(&qt_thread, NetworkClientState::default());
    }

    loop {
        tokio::select! {
            Some(Ok(signal)) = async {
                if let Some(ref mut s) = signal_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                let header = signal.header();
                let member = header.member();
                let member_str = member.map_or("", zbus::names::MemberName::as_str);
                if member_str == "PropertiesChanged" || member_str == "Changed" {
                    debug!("NetworkModel received signal: {member_str}");
                    if let Some(ref proxy) = network_proxy {
                        let state = fetch_network_state(proxy).await;
                        info!(
                            "NetworkModel updated: present={}, connected={}, kind={}, strength={}, ssid='{}'",
                            state.present, state.connected, state.kind, state.strength, state.ssid
                        );
                        dispatch_state(&qt_thread, state);
                    }
                }
            }

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
                        info!("kynex-daemon left the bus; resetting network state");
                        network_proxy = None;
                        dispatch_state(&qt_thread, NetworkClientState::default());
                    } else {
                        info!("kynex-daemon appeared on the bus; reconnecting NetworkModel");
                        network_proxy = create_daemon_proxy(&session_conn, &dbus_proxy).await;
                        if let Some(ref proxy) = network_proxy {
                            let state = fetch_network_state(proxy).await;
                            dispatch_state(&qt_thread, state);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_client_state_default() {
        let state = NetworkClientState::default();
        assert!(!state.present);
        assert!(!state.connected);
        assert_eq!(state.kind, 0);
        assert!(!state.wifi);
        assert!(!state.ethernet);
        assert_eq!(state.strength, 0);
        assert_eq!(state.ssid, "");
        assert!(!state.weak);
    }

    #[test]
    fn test_network_client_state_wifi_connected() {
        let state = NetworkClientState::from_raw(true, 2, 1, 80, "HomeFiber");
        assert!(state.present);
        assert!(state.connected);
        assert_eq!(state.kind, NetworkKind::Wifi as i32);
        assert!(state.wifi);
        assert!(!state.ethernet);
        assert_eq!(state.strength, 80);
        assert_eq!(state.ssid, "HomeFiber");
        assert!(!state.weak);
    }

    #[test]
    fn test_network_client_state_weak_signal_threshold() {
        // Below 25% threshold
        let weak_state = NetworkClientState::from_raw(true, 2, 1, 20, "WeakCafe");
        assert!(weak_state.present);
        assert!(weak_state.connected);
        assert!(weak_state.weak);

        // At or above 25% threshold
        let strong_state = NetworkClientState::from_raw(true, 2, 1, 25, "OkayCafe");
        assert!(!strong_state.weak);
    }

    #[test]
    fn test_network_client_state_ethernet() {
        let state = NetworkClientState::from_raw(true, 2, 2, 0, "");
        assert!(state.present);
        assert!(state.connected);
        assert_eq!(state.kind, NetworkKind::Ethernet as i32);
        assert_eq!(state.strength, 0);
        assert!(!state.weak);
    }

    #[test]
    fn test_network_client_state_disconnected() {
        let state = NetworkClientState::from_raw(true, 0, 1, 0, "");
        assert!(state.present);
        assert!(!state.connected);
        assert_eq!(state.kind, NetworkKind::Wifi as i32);
        assert!(!state.weak);
    }

    #[test]
    fn test_network_client_state_clamped_and_fallback() {
        let state = NetworkClientState::from_raw(true, 99, 99, 500, "Weird");
        assert!(state.present);
        assert!(!state.connected); // Unknown state != Connected
        assert_eq!(state.kind, NetworkKind::None as i32); // Fallback to None
        assert_eq!(state.strength, 100); // Clamped to 100
        assert!(!state.weak);
    }

    #[test]
    fn test_network_client_state_not_present() {
        let state = NetworkClientState::from_raw(false, 2, 1, 90, "Invisible");
        assert_eq!(state, NetworkClientState::default());
    }
}
