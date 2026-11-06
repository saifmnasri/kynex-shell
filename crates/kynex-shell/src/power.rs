//! Power management client model exposing battery state to QML via `cxx-qt`.

#![allow(clippy::float_cmp)]

use core::pin::Pin;
use cxx_qt::Threading;
use futures_util::StreamExt;
use kynex_ipc::power::{BatteryState, Power1Proxy, POWER_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use tracing::{debug, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::names::WellKnownName;
use zbus::{MatchRule, MessageStream};

/// Pure logic representation of the battery status.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PowerState {
    /// Charge percentage clamped between 0.0 and 100.0.
    pub percentage: f64,
    /// Whether the battery is actively charging or pending charge.
    pub charging: bool,
    /// Whether a battery device is detected and present.
    pub present: bool,
    /// Whether the battery level is low (below threshold).
    pub low: bool,
}

impl PowerState {
    /// Constructs a validated `PowerState` from raw D-Bus values.
    #[must_use]
    pub fn from_raw(present: bool, raw_percentage: f64, raw_state: u32) -> Self {
        let percentage =
            if raw_percentage.is_nan() || raw_percentage.is_infinite() || raw_percentage < 0.0 {
                0.0
            } else if raw_percentage > 100.0 {
                100.0
            } else {
                raw_percentage
            };

        let charging = BatteryState::from_u32(raw_state).is_charging();
        let low = present && percentage < kynex_common::BATTERY_LOW_PERCENTAGE;

        Self {
            percentage,
            charging,
            present,
            low,
        }
    }
}

/// CXX-Qt bridge module for the `PowerModel` `QObject`.
#[cxx_qt::bridge]
pub mod qobject {
    extern "RustQt" {
        /// Battery status model exposed to QML.
        #[qobject]
        #[qml_element]
        #[qproperty(f64, percentage)]
        #[qproperty(bool, charging)]
        #[qproperty(bool, present)]
        #[qproperty(bool, low)]
        type PowerModel = super::PowerModelRust;

        /// Starts the background D-Bus listener connecting to `org.kynexos.Daemon`.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for PowerModel {}
}

/// Backing data for the `PowerModel` `QObject`.
#[derive(Default)]
pub struct PowerModelRust {
    percentage: f64,
    charging: bool,
    present: bool,
    low: bool,
}

impl qobject::PowerModel {
    /// Initialize the background D-Bus client connecting to `kynex-daemon`.
    pub fn init(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_power_listener(qt_thread).await;
        });
    }
}

/// Reads the current snapshot from `Power1Proxy`, converting to `PowerState`.
async fn fetch_power_state(proxy: &Power1Proxy<'_>) -> PowerState {
    let present = proxy.present().await.unwrap_or(false);
    let percentage = proxy.percentage().await.unwrap_or(0.0);
    let state = proxy.state().await.unwrap_or(0);
    PowerState::from_raw(present, percentage, state)
}

/// Dispatches a `PowerState` update to the Qt main thread via `CxxQtThread`.
fn dispatch_state(qt_thread: &cxx_qt::CxxQtThread<qobject::PowerModel>, state: PowerState) {
    let _ = qt_thread.queue(move |mut qobj| {
        qobj.as_mut().set_percentage(state.percentage);
        qobj.as_mut().set_charging(state.charging);
        qobj.as_mut().set_present(state.present);
        qobj.as_mut().set_low(state.low);
    });
}

/// Attempts to connect to `org.kynexos.Daemon` if present on the bus.
async fn create_daemon_proxy<'a>(
    conn: &'a zbus::Connection,
    dbus_proxy: &DBusProxy<'_>,
) -> Option<Power1Proxy<'a>> {
    let wk_name: Result<WellKnownName<'_>, _> = DAEMON_DESTINATION.try_into();
    let has_daemon = match wk_name {
        Ok(ref name) => dbus_proxy
            .get_name_owner(zbus::names::BusName::WellKnown(name.as_ref()))
            .await
            .is_ok(),
        Err(_) => false,
    };

    if has_daemon {
        Power1Proxy::builder(conn)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .ok()
    } else {
        None
    }
}

/// Initializes a match-rule signal stream on `POWER_PATH`.
async fn init_signal_stream(conn: &zbus::Connection) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path(POWER_PATH)
        .build()
        .ok()?;
    MessageStream::for_match_rule(rule, conn, None).await.ok()
}

/// Background asynchronous listener connecting to `org.kynexos.Daemon.Power1`.
///
/// Signal-driven: listens for property updates and reconnects automatically
/// when `kynex-daemon` restarts without blocking the Qt GUI thread.
pub async fn run_power_listener(qt_thread: cxx_qt::CxxQtThread<qobject::PowerModel>) {
    let session_conn = match zbus::connection::Builder::session().build().await {
        Ok(c) => c,
        Err(err) => {
            warn!("PowerModel failed to connect to session bus: {err}");
            dispatch_state(&qt_thread, PowerState::default());
            return;
        }
    };

    let dbus_proxy = match DBusProxy::new(&session_conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("PowerModel failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
        .ok();

    let mut signal_stream = init_signal_stream(&session_conn).await;
    let mut power_proxy = create_daemon_proxy(&session_conn, &dbus_proxy).await;

    if let Some(ref proxy) = power_proxy {
        let state = fetch_power_state(proxy).await;
        info!(
            "PowerModel connected to daemon: present={}, percentage={}%",
            state.present, state.percentage
        );
        dispatch_state(&qt_thread, state);
    } else {
        info!("kynex-daemon not found on bus; setting present=false");
        dispatch_state(&qt_thread, PowerState::default());
    }

    loop {
        tokio::select! {
            // Signal from kynex-daemon on /org/kynexos/Daemon
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
                    debug!("PowerModel received signal: {member_str}");
                    if let Some(ref proxy) = power_proxy {
                        let state = fetch_power_state(proxy).await;
                        info!(
                            "PowerModel updated: present={}, percentage={}%, charging={}",
                            state.present, state.percentage, state.charging
                        );
                        dispatch_state(&qt_thread, state);
                    }
                }
            }

            // Daemon appeared or disappeared
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
                        info!("kynex-daemon disconnected; resetting power state");
                        power_proxy = None;
                        dispatch_state(&qt_thread, PowerState::default());
                    } else {
                        info!("kynex-daemon connected; fetching battery state");
                        power_proxy = Power1Proxy::builder(&session_conn)
                            .cache_properties(zbus::proxy::CacheProperties::No)
                            .build()
                            .await
                            .ok();
                        if let Some(ref proxy) = power_proxy {
                            let state = fetch_power_state(proxy).await;
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
    fn test_power_state_sanitization() {
        // Normal in-range values
        let state = PowerState::from_raw(true, 75.0, 1);
        assert!(state.present);
        assert_eq!(state.percentage, 75.0);
        assert!(state.charging);
        assert!(!state.low);

        // Lower-bound clamping
        let state_low = PowerState::from_raw(true, -10.0, 2);
        assert_eq!(state_low.percentage, 0.0);
        assert!(!state_low.charging);
        assert!(state_low.low);

        // Upper-bound clamping
        let state_high = PowerState::from_raw(true, 150.0, 2);
        assert_eq!(state_high.percentage, 100.0);
        assert!(!state_high.low);

        // Low threshold boundary checks
        let state_boundary_low = PowerState::from_raw(true, 19.9, 2);
        assert!(state_boundary_low.low);
        let state_boundary_ok = PowerState::from_raw(true, 20.0, 2);
        assert!(!state_boundary_ok.low);

        // NaN handling
        let state_nan = PowerState::from_raw(true, f64::NAN, 0);
        assert_eq!(state_nan.percentage, 0.0);
        assert!(state_nan.low);

        // Infinite handling
        let state_inf = PowerState::from_raw(true, f64::INFINITY, 0);
        assert_eq!(state_inf.percentage, 0.0);
        let state_neginf = PowerState::from_raw(true, f64::NEG_INFINITY, 0);
        assert_eq!(state_neginf.percentage, 0.0);
    }

    #[test]
    fn test_power_state_charging_states() {
        // Charging states: 1 (Charging), 5 (PendingCharge)
        assert!(PowerState::from_raw(true, 50.0, 1).charging);
        assert!(PowerState::from_raw(true, 50.0, 5).charging);

        // Non-charging states: 0 (Unknown), 2 (Discharging), 3 (Empty), 4 (FullyCharged), 6 (PendingDischarge), 99 (invalid)
        assert!(!PowerState::from_raw(true, 50.0, 0).charging);
        assert!(!PowerState::from_raw(true, 50.0, 2).charging);
        assert!(!PowerState::from_raw(true, 50.0, 3).charging);
        assert!(!PowerState::from_raw(true, 50.0, 4).charging);
        assert!(!PowerState::from_raw(true, 50.0, 6).charging);
        assert!(!PowerState::from_raw(true, 50.0, 99).charging);
    }

    #[test]
    fn test_power_state_not_present() {
        let state = PowerState::from_raw(false, 0.0, 0);
        assert!(!state.present);
        assert_eq!(state.percentage, 0.0);
        assert!(!state.charging);
        assert!(!state.low);
    }
}
