//! Window tracking and `KWin` feeder service implementing `WindowsFeed1` and `Windows1`.
//!
//! Enforces caller authentication via `SenderPolicy` to prevent spoofing from untrusted
//! session bus clients, applies strict rate limiting (200 calls/s) per sender, and
//! dispatches atomic updates to shell clients with zero polling.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use kynex_ipc::windows::WindowItem;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection};

use crate::window_state::{UpsertResult, WindowState};

/// Default maximum allowed feeder calls per second from any single sender.
pub const FEED_RATE_LIMIT_PER_SEC: u32 = 200;

/// Policy interface defining sender authorization checks.
pub trait SenderPolicy: std::fmt::Debug + Send + Sync {
    /// Verifies whether the caller with the given UID and PID is authorized to feed window events.
    ///
    /// # Errors
    /// Returns `Err` with a diagnostic explanation if caller is unauthorized or inspection fails.
    fn verify_sender(&self, uid: u32, pid: u32) -> Result<(), String>;
}

/// Production sender policy: requires session UID match and canonical executable `kwin_wayland`.
#[derive(Debug, Clone)]
pub struct KWinSenderPolicy {
    daemon_uid: u32,
}

impl KWinSenderPolicy {
    /// Creates a new `KWinSenderPolicy` bound to the current process UID.
    #[must_use]
    pub fn new() -> Self {
        let daemon_uid = rustix::process::getuid().as_raw();
        Self { daemon_uid }
    }
}

impl Default for KWinSenderPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl SenderPolicy for KWinSenderPolicy {
    fn verify_sender(&self, uid: u32, pid: u32) -> Result<(), String> {
        if uid != self.daemon_uid {
            return Err(format!(
                "UID mismatch: caller UID {uid} does not match daemon UID {}",
                self.daemon_uid
            ));
        }

        let exe_link = format!("/proc/{pid}/exe");
        match std::fs::read_link(&exe_link) {
            Ok(exe_path) => {
                if exe_path != std::path::Path::new("/usr/bin/kwin_wayland")
                    && exe_path.file_name() != Some(std::ffi::OsStr::new("kwin_wayland"))
                {
                    let display_path = exe_path.display();
                    return Err(format!(
                        "Unauthorized caller executable '{display_path}' for PID {pid}: expected '/usr/bin/kwin_wayland'"
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                // Binaries with Linux file capabilities (such as /usr/bin/kwin_wayland with cap_sys_nice=ep)
                // are marked non-dumpable by the kernel, restricting /proc/<pid>/exe symlink resolution
                // even among processes sharing the same UID. Fall back to /proc/<pid>/comm and cmdline.
                let comm_path = format!("/proc/{pid}/comm");
                let comm = std::fs::read_to_string(&comm_path)
                    .map_err(|err| format!("Failed to read comm for PID {pid}: {err}"))?;
                if comm.trim() != "kwin_wayland" {
                    return Err(format!(
                        "Unauthorized caller comm {:?} for PID {pid}: expected 'kwin_wayland'",
                        comm.trim()
                    ));
                }

                let cmdline_path = format!("/proc/{pid}/cmdline");
                let cmdline = std::fs::read(&cmdline_path)
                    .map_err(|err| format!("Failed to read cmdline for PID {pid}: {err}"))?;
                let first_arg = cmdline
                    .split(|&b| b == 0)
                    .next()
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .unwrap_or_default();
                if !first_arg.ends_with("kwin_wayland") {
                    return Err(format!(
                        "Unauthorized caller cmdline '{first_arg}' for PID {pid}: expected 'kwin_wayland'"
                    ));
                }
            }
            Err(e) => {
                return Err(format!("Failed to inspect caller exe for PID {pid}: {e}"));
            }
        }

        Ok(())
    }
}

/// Permissive policy for unit and integration testing.
#[derive(Debug, Clone, Default)]
pub struct AllowAllSenderPolicy;

impl SenderPolicy for AllowAllSenderPolicy {
    fn verify_sender(&self, _uid: u32, _pid: u32) -> Result<(), String> {
        Ok(())
    }
}

/// Rate limiting bucket for a single sender bus name.
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

/// Shared internal state holding window data, sender authorization policy, and rate limiter.
#[derive(Debug)]
pub struct WindowsSharedState {
    /// Tracked windows state machine.
    pub state: RwLock<WindowState>,
    /// Pluggable sender authorization policy.
    pub policy: Arc<dyn SenderPolicy>,
    /// Rate limiters mapped by sender bus name.
    rate_limiters: Mutex<HashMap<String, RateLimitBucket>>,
    /// Counter of dropped calls due to rate limits or validation errors.
    pub dropped_calls: AtomicU64,
}

impl WindowsSharedState {
    /// Creates a new `WindowsSharedState` with the provided policy.
    #[must_use]
    pub fn new(policy: Arc<dyn SenderPolicy>) -> Self {
        Self {
            state: RwLock::new(WindowState::new()),
            policy,
            rate_limiters: Mutex::new(HashMap::new()),
            dropped_calls: AtomicU64::new(0),
        }
    }

    /// Verifies sender credentials and enforces rate limiting.
    ///
    /// # Errors
    /// Returns `Err` with D-Bus access denied or limits exceeded errors.
    pub async fn check_caller(
        &self,
        sender: &str,
        uid: u32,
        pid: u32,
    ) -> Result<(), zbus::fdo::Error> {
        if let Err(err) = self.policy.verify_sender(uid, pid) {
            warn!("Rejected unauthorized feed caller '{sender}' (PID {pid}, UID {uid}): {err}");
            self.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return Err(zbus::fdo::Error::AccessDenied(err));
        }

        let now = Instant::now();
        let mut buckets = self.rate_limiters.lock().await;
        let bucket = buckets
            .entry(sender.to_string())
            .or_insert_with(|| RateLimitBucket::new(now));

        if !bucket.check_and_inc(now, FEED_RATE_LIMIT_PER_SEC) {
            warn!(
                "Rate limit exceeded for sender '{sender}' (> {FEED_RATE_LIMIT_PER_SEC} calls/s)"
            );
            self.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return Err(zbus::fdo::Error::LimitsExceeded(format!(
                "Rate limit exceeded (max {FEED_RATE_LIMIT_PER_SEC} calls/s)"
            )));
        }

        Ok(())
    }
}

/// D-Bus object implementing `org.kynexos.Daemon.Windows1` (public read-only interface for shell).
#[derive(Debug, Clone)]
pub struct WindowsServer {
    shared: Arc<WindowsSharedState>,
}

impl WindowsServer {
    /// Creates a new `WindowsServer` sharing state with the feeder.
    #[must_use]
    pub fn new(shared: Arc<WindowsSharedState>) -> Self {
        Self { shared }
    }

    /// Returns a copy of all currently tracked windows.
    pub async fn snapshot(&self) -> Vec<WindowItem> {
        self.shared.state.read().await.list().to_vec()
    }

    /// Clears all tracked windows (e.g. when `KWin` disconnects) and emits removal signals.
    pub async fn clear(&self, emitter: &SignalEmitter<'_>) {
        let (removed_windows, had_active) = {
            let mut state = self.shared.state.write().await;
            let list = state.list().to_vec();
            let had_active = state.active_id().is_some();
            state.reset();
            (list, had_active)
        };

        for win in removed_windows {
            let _ = Self::removed(emitter, &win.id).await;
        }

        if had_active {
            let _ = Self::active_changed(emitter, "").await;
        }
    }
}

#[interface(name = "org.kynexos.Daemon.Windows1")]
impl WindowsServer {
    /// Returns the complete array of currently tracked windows.
    async fn list(&self) -> Vec<WindowItem> {
        self.shared.state.read().await.list().to_vec()
    }

    /// Signal emitted when a new window is added.
    #[zbus(signal)]
    pub async fn added(
        emitter: &SignalEmitter<'_>,
        id: &str,
        title: &str,
        app_id: &str,
        minimized: bool,
        active: bool,
    ) -> zbus::Result<()>;

    /// Signal emitted when a window's title, `app_id`, or minimized state changes.
    #[zbus(signal)]
    pub async fn changed(
        emitter: &SignalEmitter<'_>,
        id: &str,
        title: &str,
        app_id: &str,
        minimized: bool,
        active: bool,
    ) -> zbus::Result<()>;

    /// Signal emitted when a window is closed/removed.
    #[zbus(signal)]
    pub async fn removed(emitter: &SignalEmitter<'_>, id: &str) -> zbus::Result<()>;

    /// Signal emitted when the active window changes.
    #[zbus(signal)]
    pub async fn active_changed(emitter: &SignalEmitter<'_>, id: &str) -> zbus::Result<()>;
}

/// D-Bus object implementing `org.kynexos.Daemon.WindowsFeed1` (private feeder interface for `KWin`).
#[derive(Debug, Clone)]
pub struct WindowsFeedServer {
    shared: Arc<WindowsSharedState>,
    signal_emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
}

impl WindowsFeedServer {
    /// Creates a new `WindowsFeedServer` sharing state with `WindowsServer`.
    #[must_use]
    pub fn new(shared: Arc<WindowsSharedState>) -> Self {
        Self {
            shared,
            signal_emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Sets the `SignalEmitter` used to broadcast updates on `Windows1`.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        *self.signal_emitter.lock().await = Some(emitter);
    }
}

#[interface(name = "org.kynexos.Daemon.WindowsFeed1")]
impl WindowsFeedServer {
    /// Clears all tracked windows.
    async fn reset(
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

        self.shared.check_caller(sender.as_str(), uid, pid).await?;

        let (removed_windows, had_active) = {
            let mut state = self.shared.state.write().await;
            let list = state.list().to_vec();
            let had_active = state.active_id().is_some();
            state.reset();
            (list, had_active)
        };

        let emitter_guard = self.signal_emitter.lock().await;
        if let Some(ref emitter) = *emitter_guard {
            for win in removed_windows {
                let _ = WindowsServer::removed(emitter, &win.id).await;
            }
            if had_active {
                let _ = WindowsServer::active_changed(emitter, "").await;
            }
        }

        info!("WindowsFeed1: state reset by {sender}");
        Ok(())
    }

    /// Inserts or updates window metadata.
    #[allow(clippy::too_many_arguments)]
    async fn upsert(
        &self,
        id: String,
        title: String,
        app_id: String,
        minimized: bool,
        active: bool,
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

        self.shared.check_caller(sender.as_str(), uid, pid).await?;

        let res = {
            let mut state = self.shared.state.write().await;
            state.upsert(&id, &title, &app_id, minimized, active)
        };

        match res {
            UpsertResult::Added(item) => {
                debug!(
                    "WindowsFeed1: added window '{}' ('{}')",
                    item.id, item.title
                );
                let emitter_guard = self.signal_emitter.lock().await;
                if let Some(ref emitter) = *emitter_guard {
                    let _ = WindowsServer::added(
                        emitter,
                        &item.id,
                        &item.title,
                        &item.app_id,
                        item.minimized,
                        item.active,
                    )
                    .await;
                    if item.active {
                        let _ = WindowsServer::active_changed(emitter, &item.id).await;
                    }
                }
                Ok(())
            }
            UpsertResult::Changed(item) => {
                debug!("WindowsFeed1: updated window '{}'", item.id);
                let emitter_guard = self.signal_emitter.lock().await;
                if let Some(ref emitter) = *emitter_guard {
                    let _ = WindowsServer::changed(
                        emitter,
                        &item.id,
                        &item.title,
                        &item.app_id,
                        item.minimized,
                        item.active,
                    )
                    .await;
                    if item.active {
                        let _ = WindowsServer::active_changed(emitter, &item.id).await;
                    }
                }
                Ok(())
            }
            UpsertResult::Unchanged => Ok(()),
            UpsertResult::CapacityExceeded => {
                warn!("WindowsFeed1: capacity limit exceeded (max 512 windows)");
                self.shared.dropped_calls.fetch_add(1, Ordering::Relaxed);
                Err(zbus::fdo::Error::LimitsExceeded(
                    "Window tracking capacity (512) exceeded".into(),
                ))
            }
            UpsertResult::InvalidInput(err) => {
                warn!("WindowsFeed1: invalid input from {sender}: {err}");
                self.shared.dropped_calls.fetch_add(1, Ordering::Relaxed);
                Err(zbus::fdo::Error::InvalidArgs(err))
            }
        }
    }

    /// Removes a window by ID.
    async fn remove(
        &self,
        id: String,
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

        self.shared.check_caller(sender.as_str(), uid, pid).await?;

        let removed = {
            let mut state = self.shared.state.write().await;
            state.remove(&id)
        };

        if let Some(item) = removed {
            debug!("WindowsFeed1: removed window '{}'", item.id);
            let emitter_guard = self.signal_emitter.lock().await;
            if let Some(ref emitter) = *emitter_guard {
                let _ = WindowsServer::removed(emitter, &item.id).await;
                if item.active {
                    let _ = WindowsServer::active_changed(emitter, "").await;
                }
            }
        }

        Ok(())
    }

    /// Sets the currently active window.
    async fn set_active(
        &self,
        id: String,
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

        self.shared.check_caller(sender.as_str(), uid, pid).await?;

        let changed = {
            let mut state = self.shared.state.write().await;
            state.set_active(&id)
        };

        if changed {
            debug!("WindowsFeed1: active window changed to '{id}'");
            let emitter_guard = self.signal_emitter.lock().await;
            if let Some(ref emitter) = *emitter_guard {
                let _ = WindowsServer::active_changed(emitter, &id).await;
            }
        }

        Ok(())
    }
}

/// Watches for `KWin` disappearance from session D-Bus and clears state when lost.
pub async fn run_kwin_watcher(
    conn: Connection,
    server: WindowsServer,
    emitter: SignalEmitter<'static>,
) {
    let dbus = match DBusProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to create DBusProxy for KWin watcher: {e}");
            return;
        }
    };

    let mut stream = match dbus
        .receive_name_owner_changed_with_args(&[(0, "org.kde.KWin")])
        .await
    {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to watch org.kde.KWin name owner changes: {e}");
            return;
        }
    };

    while let Some(signal) = stream.next().await {
        if let Ok(args) = signal.args() {
            let new_owner = args.new_owner();
            if new_owner.as_ref().is_none_or(|o| o.as_str().is_empty()) {
                info!("KWin left the bus; clearing tracked windows");
                server.clear(&emitter).await;
            }
        }
    }
}
