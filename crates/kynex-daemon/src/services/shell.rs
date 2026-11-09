//! Shell coordination service implementing `org.kynexos.Daemon.Shell1` and `org.kynexos.Daemon.ShellFeed1`.
//!
//! Provides global shortcut handling from the `KWin` compositor script, protected by the exact same
//! layered sender authentication policy as `WindowsFeed1`, and broadcasts `LauncherToggleRequested`
//! to desktop surface components.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::names::BusName;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection};

use super::windows::SenderPolicy;

/// Maximum allowed shortcut calls per second from `KWin` feeder (rate limiting guard).
pub const SHELL_FEED_RATE_LIMIT_PER_SEC: u32 = 20;

/// Internal sliding window rate limiter for sender throttling.
#[derive(Debug)]
struct RateLimitBucket {
    window_start: Instant,
    count: u32,
}

impl RateLimitBucket {
    fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            count: 0,
        }
    }

    fn check_and_inc(&mut self, now: Instant, limit_per_sec: u32) -> bool {
        let elapsed = now.duration_since(self.window_start);
        if elapsed >= std::time::Duration::from_secs(1) {
            self.window_start = now;
            self.count = 1;
            true
        } else if self.count < limit_per_sec {
            self.count += 1;
            true
        } else {
            false
        }
    }
}

/// Shared state for shell feed and shell public service.
#[derive(Debug)]
pub struct ShellSharedState {
    /// Layered feeder authentication policy.
    pub policy: Arc<dyn SenderPolicy>,
    /// Unique bus name of currently tracked `org.kde.KWin` owner.
    pub tracked_kwin_owner: RwLock<Option<String>>,
    /// Per-sender rate limiting buckets.
    rate_limiters: Mutex<HashMap<String, RateLimitBucket>>,
    /// Total dropped unauthorized or rate-limited calls.
    pub dropped_calls: AtomicU64,
}

impl ShellSharedState {
    /// Creates a new `ShellSharedState` with the specified sender policy.
    #[must_use]
    pub fn new(policy: Arc<dyn SenderPolicy>) -> Self {
        Self {
            policy,
            tracked_kwin_owner: RwLock::new(None),
            rate_limiters: Mutex::new(HashMap::new()),
            dropped_calls: AtomicU64::new(0),
        }
    }

    /// Verifies caller credentials against sender policy and enforces rate limit.
    ///
    /// # Errors
    /// Returns `AccessDenied` if authentication fails, or `LimitsExceeded` if rate limit is exceeded.
    pub async fn check_caller(
        &self,
        sender: &str,
        uid: u32,
        pid: u32,
        current_kwin_owner: Option<&str>,
    ) -> Result<(), zbus::fdo::Error> {
        if let Err(err) = self
            .policy
            .verify_sender(sender, uid, pid, current_kwin_owner)
        {
            warn!(
                "Rejected unauthorized shell feed caller '{sender}' (PID {pid}, UID {uid}): {err}"
            );
            self.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return Err(zbus::fdo::Error::AccessDenied(err));
        }

        let now = Instant::now();
        let mut buckets = self.rate_limiters.lock().await;
        let bucket = buckets
            .entry(sender.to_string())
            .or_insert_with(|| RateLimitBucket::new(now));

        if !bucket.check_and_inc(now, SHELL_FEED_RATE_LIMIT_PER_SEC) {
            warn!("Rate limit exceeded for shell feed caller '{sender}' (> {SHELL_FEED_RATE_LIMIT_PER_SEC} calls/s)");
            self.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return Err(zbus::fdo::Error::LimitsExceeded(format!(
                "Rate limit exceeded (max {SHELL_FEED_RATE_LIMIT_PER_SEC} calls/s)"
            )));
        }

        Ok(())
    }

    /// Returns count of dropped unauthorized or rate-limited calls.
    #[must_use]
    pub fn dropped_calls(&self) -> u64 {
        self.dropped_calls.load(Ordering::Relaxed)
    }
}

/// D-Bus object implementing `org.kynexos.Daemon.Shell1` (public shell events interface).
#[derive(Debug, Clone)]
pub struct ShellServer {
    shared: Arc<ShellSharedState>,
}

impl ShellServer {
    /// Creates a new `ShellServer`.
    #[must_use]
    pub fn new(shared: Arc<ShellSharedState>) -> Self {
        Self { shared }
    }

    /// Returns a reference to the shared shell state.
    #[must_use]
    pub fn shared(&self) -> &Arc<ShellSharedState> {
        &self.shared
    }
}

#[interface(name = "org.kynexos.Daemon.Shell1")]
impl ShellServer {
    /// Signal emitted when a launcher toggle is requested.
    #[zbus(signal)]
    pub async fn launcher_toggle_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// D-Bus object implementing `org.kynexos.Daemon.ShellFeed1` (private feeder interface for `KWin`).
#[derive(Debug, Clone)]
pub struct ShellFeedServer {
    shared: Arc<ShellSharedState>,
    signal_emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
}

impl ShellFeedServer {
    /// Creates a new `ShellFeedServer`.
    #[must_use]
    pub fn new(shared: Arc<ShellSharedState>) -> Self {
        Self {
            shared,
            signal_emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Sets the `SignalEmitter` used to broadcast updates on `Shell1`.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        *self.signal_emitter.lock().await = Some(emitter);
    }
}

#[interface(name = "org.kynexos.Daemon.ShellFeed1")]
impl ShellFeedServer {
    /// Receives launcher shortcut trigger from the `KWin` compositor script.
    async fn launcher_shortcut(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<()> {
        let sender = hdr
            .sender()
            .ok_or_else(|| zbus::fdo::Error::AccessDenied("Missing message sender".into()))?;

        let dbus = DBusProxy::new(conn).await?;
        let uid = dbus.get_connection_unix_user(sender.clone().into()).await?;
        let pid = dbus
            .get_connection_unix_process_id(sender.clone().into())
            .await?;
        let kwin_owner = match dbus
            .get_name_owner(BusName::from_static_str("org.kde.KWin")?)
            .await
        {
            Ok(owner) => Some(owner.to_string()),
            Err(_) => None,
        };
        *self.shared.tracked_kwin_owner.write().await = kwin_owner.clone();

        self.shared
            .check_caller(sender.as_str(), uid, pid, kwin_owner.as_deref())
            .await?;

        info!("ShellFeed1: launcher shortcut received from {sender}; requesting launcher toggle");

        let emitter_guard = self.signal_emitter.lock().await;
        if let Some(ref emitter) = *emitter_guard {
            if let Err(e) = ShellServer::launcher_toggle_requested(emitter).await {
                warn!("Failed to emit LauncherToggleRequested signal: {e}");
            }
        } else {
            warn!("ShellFeed1: no SignalEmitter registered for Shell1");
        }

        Ok(())
    }
}

/// Background task watching `org.kde.KWin` bus name ownership changes for shell service.
pub async fn run_shell_kwin_watcher(conn: Connection, shared: Arc<ShellSharedState>) {
    let dbus = match DBusProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to create DBusProxy for shell KWin watcher: {e}");
            return;
        }
    };

    if let Ok(kwin_name) = BusName::from_static_str("org.kde.KWin") {
        if let Ok(owner) = dbus.get_name_owner(kwin_name).await {
            *shared.tracked_kwin_owner.write().await = Some(owner.to_string());
        }
    }

    let mut stream = match dbus
        .receive_name_owner_changed_with_args(&[(0, "org.kde.KWin")])
        .await
    {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to watch org.kde.KWin name owner changes for shell: {e}");
            return;
        }
    };

    while let Some(signal) = stream.next().await {
        if let Ok(args) = signal.args() {
            let new_owner = args.new_owner();
            let new_owner_str = new_owner.as_ref().and_then(|o| {
                let s = o.as_str();
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            });
            *shared.tracked_kwin_owner.write().await = new_owner_str;
        }
    }
}
