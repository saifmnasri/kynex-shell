//! Power management service integrating with `UPower` `DisplayDevice`.

use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::names::WellKnownName;
use zbus::object_server::SignalEmitter;
use zbus::proxy;
use zbus::{MatchRule, MessageStream};

/// Sanitize percentage to clamp between 0.0 and 100.0, rejecting NaN/infinite values.
#[must_use]
pub fn sanitize_percentage(val: f64) -> f64 {
    if val.is_nan() || val.is_infinite() || val < 0.0 {
        0.0
    } else if val > 100.0 {
        100.0
    } else {
        val
    }
}

/// Sanitize state to ensure it is within 0..=6 (`UPower` specification).
#[must_use]
pub fn sanitize_state(val: u32) -> u32 {
    if val > 6 {
        0
    } else {
        val
    }
}

/// Sanitize time estimate to be non-negative.
#[must_use]
pub fn sanitize_time(val: i64) -> i64 {
    if val < 0 {
        0
    } else {
        val
    }
}

/// Internal state snapshot for power metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerSnapshot {
    /// Whether a battery device is present.
    pub present: bool,
    /// Percentage charged (0.0 to 100.0).
    pub percentage: f64,
    /// Battery state (0=Unknown, 1=Charging, 2=Discharging, etc.).
    pub state: u32,
    /// Seconds until battery is empty.
    pub time_to_empty: i64,
    /// Seconds until battery is full.
    pub time_to_full: i64,
}

impl Default for PowerSnapshot {
    fn default() -> Self {
        Self {
            present: false,
            percentage: 0.0,
            state: 0,
            time_to_empty: 0,
            time_to_full: 0,
        }
    }
}

/// `UPower` `DisplayDevice` proxy for reading and watching battery status.
#[proxy(
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice",
    interface = "org.freedesktop.UPower.Device"
)]
pub trait UPowerDevice {
    /// Whether device is present.
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;

    /// Battery percentage.
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    /// Battery state.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// Estimated seconds until empty.
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;

    /// Estimated seconds until full.
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
}

/// D-Bus object implementing `org.kynexos.Daemon.Power1`.
#[derive(Debug, Clone, Default)]
pub struct PowerServer {
    state: Arc<RwLock<PowerSnapshot>>,
}

impl PowerServer {
    /// Creates a new `PowerServer` instance with default values (present=false).
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(PowerSnapshot::default())),
        }
    }

    /// Returns a copy of the current `PowerSnapshot`.
    pub async fn snapshot(&self) -> PowerSnapshot {
        *self.state.read().await
    }

    /// Updates internal state and emits D-Bus `PropertiesChanged` signals if changed.
    pub async fn update(&self, new_state: PowerSnapshot, emitter: Option<&SignalEmitter<'_>>) {
        let (old, changed) = {
            let mut guard = self.state.write().await;
            let old = *guard;
            if old == new_state {
                (old, false)
            } else {
                *guard = new_state;
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
            if (old.percentage - new_state.percentage).abs() > f64::EPSILON {
                let _ = self.percentage_changed(em).await;
            }
            if old.state != new_state.state {
                let _ = self.state_changed(em).await;
            }
            if old.time_to_empty != new_state.time_to_empty {
                let _ = self.time_to_empty_changed(em).await;
            }
            if old.time_to_full != new_state.time_to_full {
                let _ = self.time_to_full_changed(em).await;
            }
        }
    }
}

#[zbus::interface(name = "org.kynexos.Daemon.Power1")]
impl PowerServer {
    /// Whether a battery device is detected and present.
    #[zbus(property)]
    async fn present(&self) -> bool {
        self.state.read().await.present
    }

    /// Battery percentage level clamped between 0.0 and 100.0.
    #[zbus(property)]
    async fn percentage(&self) -> f64 {
        self.state.read().await.percentage
    }

    /// Operational state code (0=Unknown, 1=Charging, 2=Discharging, etc.).
    #[zbus(property)]
    async fn state(&self) -> u32 {
        self.state.read().await.state
    }

    /// Estimated seconds remaining until battery is depleted.
    #[zbus(property)]
    async fn time_to_empty(&self) -> i64 {
        self.state.read().await.time_to_empty
    }

    /// Estimated seconds remaining until battery is fully charged.
    #[zbus(property)]
    async fn time_to_full(&self) -> i64 {
        self.state.read().await.time_to_full
    }
}

/// Reads the current snapshot from a `UPowerDeviceProxy`, sanitizing all values.
pub async fn read_device_snapshot(proxy: &UPowerDeviceProxy<'_>) -> PowerSnapshot {
    let is_present = proxy.is_present().await.unwrap_or(false);
    let raw_pct = proxy.percentage().await.unwrap_or(0.0);
    let raw_state = proxy.state().await.unwrap_or(0);
    let time_empty = proxy.time_to_empty().await.unwrap_or(0);
    let time_full = proxy.time_to_full().await.unwrap_or(0);

    PowerSnapshot {
        present: is_present,
        percentage: sanitize_percentage(raw_pct),
        state: sanitize_state(raw_state),
        time_to_empty: sanitize_time(time_empty),
        time_to_full: sanitize_time(time_full),
    }
}

/// Creates a `UPowerDeviceProxy` for the given service name on the connection.
pub async fn make_device_proxy<'a>(
    conn: &'a zbus::Connection,
    service_name: &'a str,
    path: &'a str,
) -> Option<UPowerDeviceProxy<'a>> {
    UPowerDeviceProxy::builder(conn)
        .destination(service_name)
        .path(path)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()
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

/// Initializes a match-rule signal stream on the given object path.
async fn init_device_stream(conn: &zbus::Connection, path: &str) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path(path)
        .build()
        .ok()?;
    MessageStream::for_match_rule(rule, conn, None).await.ok()
}

/// Watches `UPower` service events on the given connection and updates the `PowerServer`.
///
/// Signal-driven: connects via D-Bus signals, never polls. If `UPower` or a battery is missing,
/// sets `present = false` without panicking. Reconnects automatically if `UPower` restarts.
pub async fn run_power_service(
    upower_conn: zbus::Connection,
    server: PowerServer,
    emitter: Option<SignalEmitter<'static>>,
    upower_service: Option<String>,
) {
    let service_name = upower_service.unwrap_or_else(|| "org.freedesktop.UPower".to_string());
    let device_path = "/org/freedesktop/UPower/devices/DisplayDevice";

    // Watch NameOwnerChanged on the bus to detect UPower appearing / disappearing
    let dbus_proxy = match DBusProxy::new(&upower_conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("Failed to create DBusProxy for name owner monitoring: {err}");
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, service_name.as_str())])
        .await
        .ok();

    // Subscribe once to signals on the DisplayDevice path before initial check
    let mut device_signal_stream = init_device_stream(&upower_conn, device_path).await;

    // Initial check
    let has_owner = check_service_owner(&dbus_proxy, &service_name).await;
    let mut device_proxy = if has_owner {
        make_device_proxy(&upower_conn, &service_name, device_path).await
    } else {
        None
    };

    if let Some(ref proxy) = device_proxy {
        let snap = read_device_snapshot(proxy).await;
        info!(
            "Initial UPower state: present={}, percentage={}%",
            snap.present, snap.percentage
        );
        server.update(snap, emitter.as_ref()).await;
    } else {
        info!("UPower not available on bus; marking battery present=false");
        server
            .update(PowerSnapshot::default(), emitter.as_ref())
            .await;
    }

    loop {
        tokio::select! {
            // Signal from UPower device
            Some(Ok(signal)) = async {
                if let Some(ref mut s) = device_signal_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                let header = signal.header();
                let member = header.member();
                let member_str = member.map_or("", zbus::names::MemberName::as_str);
                if member_str == "PropertiesChanged" || member_str == "Changed" {
                    debug!("Received UPower signal: {member_str}");
                    if let Some(ref proxy) = device_proxy {
                        let snap = read_device_snapshot(proxy).await;
                        server.update(snap, emitter.as_ref()).await;
                    }
                }
            }

            // NameOwnerChanged event
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
                        info!("UPower service left the bus; resetting power state");
                        device_proxy = None;
                        server.update(PowerSnapshot::default(), emitter.as_ref()).await;
                    } else {
                        info!("UPower service appeared on the bus; reconnecting");
                        device_proxy = make_device_proxy(&upower_conn, &service_name, device_path).await;
                        if let Some(ref proxy) = device_proxy {
                            let snap = read_device_snapshot(proxy).await;
                            server.update(snap, emitter.as_ref()).await;
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
    fn test_sanitize_percentage() {
        assert_eq!(sanitize_percentage(50.0), 50.0);
        assert_eq!(sanitize_percentage(0.0), 0.0);
        assert_eq!(sanitize_percentage(100.0), 100.0);
        assert_eq!(sanitize_percentage(-0.1), 0.0);
        assert_eq!(sanitize_percentage(-100.0), 0.0);
        assert_eq!(sanitize_percentage(100.1), 100.0);
        assert_eq!(sanitize_percentage(200.0), 100.0);
        assert_eq!(sanitize_percentage(f64::NAN), 0.0);
        assert_eq!(sanitize_percentage(f64::INFINITY), 0.0);
        assert_eq!(sanitize_percentage(f64::NEG_INFINITY), 0.0);
    }

    #[test]
    fn test_sanitize_state() {
        for valid in 0..=6 {
            assert_eq!(sanitize_state(valid), valid);
        }
        assert_eq!(sanitize_state(7), 0);
        assert_eq!(sanitize_state(100), 0);
        assert_eq!(sanitize_state(u32::MAX), 0);
    }

    #[test]
    fn test_sanitize_time() {
        assert_eq!(sanitize_time(0), 0);
        assert_eq!(sanitize_time(3600), 3600);
        assert_eq!(sanitize_time(-1), 0);
        assert_eq!(sanitize_time(-1000), 0);
        assert_eq!(sanitize_time(i64::MIN), 0);
    }

    #[tokio::test]
    async fn test_power_server_snapshot_update() {
        let server = PowerServer::new();
        let initial = server.snapshot().await;
        assert_eq!(initial, PowerSnapshot::default());
        assert!(!initial.present);

        let new_snap = PowerSnapshot {
            present: true,
            percentage: 85.5,
            state: 1,
            time_to_empty: 0,
            time_to_full: 1800,
        };
        server.update(new_snap, None).await;

        let updated = server.snapshot().await;
        assert_eq!(updated, new_snap);
        assert!(updated.present);
        assert_eq!(updated.percentage, 85.5);
        assert_eq!(updated.state, 1);
    }
}
