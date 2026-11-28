//! Session lifecycle and system power management service.
//!
//! Implements `org.kynexos.Daemon.Session1` at `/org/kynexos/Daemon/Session`.
//! Manages graceful user session logout via `systemd --user` target teardown and
//! delegates system power actions (Restart, `PowerOff`, Suspend) to `systemd-logind`
//! (`org.freedesktop.login1`) with non-interactive execution (`interactive = false`).
//!
//! # Security Invariants
//! - Enforces a daemon-wide rate limit of 1 action per 5 seconds to prevent `DoS`.
//! - Enforces a hard 2-second timeout on all upstream IPC calls.
//! - Logs *only* action kind (`logout`, `restart`, `poweroff`, `suspend`), NEVER caller arguments.
//! - Strictly sanitizes all logind/systemd error strings before returning or emitting them.
//! - Gracefully degrades (`Can* = false`) when upstream services are unavailable.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use kynex_ipc::session::{SESSION_RATE_LIMIT_SECS, SESSION_TIMEOUT_SECS};
use kynex_ipc::BoxFuture;
use tokio::sync::Mutex;
use tracing::{info, warn};
use zbus::object_server::SignalEmitter;
use zbus::Connection;

/// Hard timeout for all upstream D-Bus calls.
pub const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(SESSION_TIMEOUT_SECS);

/// Rate limit duration between power actions.
pub const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(SESSION_RATE_LIMIT_SECS);

/// Sanitizes raw error text from logind or systemd before exposing it across IPC.
///
/// Strips filesystem paths, socket names, memory addresses, and maps known
/// error classes to human-readable explanations.
#[must_use]
pub fn sanitize_error_text(raw: &str) -> String {
    if raw.contains("OperationInhibited") || raw.contains("inhibit") || raw.contains("Inhibited") {
        return "An application is preventing this action".to_string();
    }
    if raw.contains("AccessDenied")
        || raw.contains("NotAuthorized")
        || raw.contains("Interactive authentication")
        || raw.contains("interactive")
        || raw.contains("Permission denied")
    {
        return "Action refused: permission denied by system policy".to_string();
    }
    if raw.contains("Timed out")
        || raw.contains("Timeout")
        || raw.contains("timed out")
        || raw.contains("deadline has elapsed")
    {
        return "Action timed out: system service did not respond in 2 seconds".to_string();
    }
    if raw.contains("Rate limit") || raw.contains("rate-limit") {
        return "Rate limit exceeded: please wait 5 seconds between power actions".to_string();
    }
    if raw.contains("ServiceUnknown")
        || raw.contains("NameHasNoOwner")
        || raw.contains("No such")
        || raw.contains("not found")
    {
        return "System service is unavailable".to_string();
    }

    // Generic sanitization: strip paths (/...), hex addresses (0x...), and non-alphanumeric punctuation
    let mut clean = String::with_capacity(raw.len());
    let mut in_path = false;
    for word in raw.split_whitespace() {
        if word.starts_with('/') || word.contains("unix:path=") {
            in_path = true;
            clean.push_str("[path] ");
            continue;
        }
        if word.starts_with("0x") {
            clean.push_str("[addr] ");
            continue;
        }
        for ch in word.chars() {
            if ch.is_alphanumeric() || ch == ' ' || ch == '.' || ch == ',' || ch == '-' || ch == ':'
            {
                clean.push(ch);
            }
        }
        clean.push(' ');
    }

    let trimmed = clean.trim();
    if trimmed.is_empty() || (in_path && trimmed == "[path]") {
        "Action failed".to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

/// Global rate limiter enforcing a sliding cooldown between destructive power actions.
#[derive(Debug)]
pub struct SessionRateLimiter {
    last_action: Mutex<Option<Instant>>,
    window: Duration,
}

impl SessionRateLimiter {
    /// Creates a rate limiter with the specified cooldown window.
    #[must_use]
    pub fn new(window: Duration) -> Self {
        Self {
            last_action: Mutex::new(None),
            window,
        }
    }

    /// Default production rate limiter (5 seconds).
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(RATE_LIMIT_WINDOW)
    }

    /// Checks if an action is permitted. If permitted, records the timestamp and returns `true`.
    pub async fn check_and_record(&self) -> bool {
        let mut last = self.last_action.lock().await;
        let now = Instant::now();
        if let Some(prev) = *last {
            if now.duration_since(prev) < self.window {
                return false;
            }
        }
        *last = Some(now);
        true
    }
}

/// Upstream logind client proxy interface.
#[zbus::proxy(
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1",
    interface = "org.freedesktop.login1.Manager"
)]
pub trait Login1Manager {
    /// Queries whether reboot is supported.
    fn can_reboot(&self) -> zbus::Result<String>;
    /// Queries whether power off is supported.
    fn can_power_off(&self) -> zbus::Result<String>;
    /// Queries whether suspend is supported.
    fn can_suspend(&self) -> zbus::Result<String>;
    /// Requests reboot.
    fn reboot(&self, interactive: bool) -> zbus::Result<()>;
    /// Requests power off.
    fn power_off(&self, interactive: bool) -> zbus::Result<()>;
    /// Requests suspend.
    fn suspend(&self, interactive: bool) -> zbus::Result<()>;
}

/// Upstream systemd user manager proxy interface.
#[zbus::proxy(
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1",
    interface = "org.freedesktop.systemd1.Manager"
)]
pub trait Systemd1Manager {
    /// Orders unit stop.
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::OwnedObjectPath>;
}

/// Abstract interface for `systemd-logind` power management interactions.
pub trait LogindClient: Send + Sync + std::fmt::Debug {
    /// Queries whether system reboot is authorized.
    fn can_reboot(&self) -> BoxFuture<'_, Result<bool, String>>;
    /// Queries whether system poweroff is authorized.
    fn can_power_off(&self) -> BoxFuture<'_, Result<bool, String>>;
    /// Queries whether system suspend is authorized.
    fn can_suspend(&self) -> BoxFuture<'_, Result<bool, String>>;
    /// Requests machine reboot with non-interactive flag.
    fn reboot(&self, interactive: bool) -> BoxFuture<'_, Result<(), String>>;
    /// Requests machine poweroff with non-interactive flag.
    fn power_off(&self, interactive: bool) -> BoxFuture<'_, Result<(), String>>;
    /// Requests machine suspend with non-interactive flag.
    fn suspend(&self, interactive: bool) -> BoxFuture<'_, Result<(), String>>;
}

/// Abstract interface for session unit teardown.
pub trait SystemdClient: Send + Sync + std::fmt::Debug {
    /// Orders clean teardown of `kynex-session.target`.
    fn stop_session_target(&self) -> BoxFuture<'_, Result<(), String>>;
}

/// Production implementation of `LogindClient` using `zbus`.
#[derive(Clone, Debug)]
pub struct ZbusLogindClient {
    conn: Arc<tokio::sync::RwLock<Option<Connection>>>,
    service_name: String,
}

impl ZbusLogindClient {
    /// Creates a new `ZbusLogindClient`.
    #[must_use]
    pub fn new(conn: Option<Connection>) -> Self {
        Self {
            conn: Arc::new(tokio::sync::RwLock::new(conn)),
            service_name: "org.freedesktop.login1".to_string(),
        }
    }

    /// Creates a new `ZbusLogindClient` with an explicit service name (for tests).
    #[must_use]
    pub fn with_service(conn: Option<Connection>, service_name: String) -> Self {
        Self {
            conn: Arc::new(tokio::sync::RwLock::new(conn)),
            service_name,
        }
    }

    /// Sets or updates the D-Bus connection.
    pub async fn set_connection(&self, conn: Connection) {
        let mut lock = self.conn.write().await;
        *lock = Some(conn);
    }
}

impl LogindClient for ZbusLogindClient {
    fn can_reboot(&self) -> BoxFuture<'_, Result<bool, String>> {
        Box::pin(async move {
            let conn = {
                let lock = self.conn.read().await;
                match lock.as_ref() {
                    Some(c) => c.clone(),
                    None => return Ok(false),
                }
            };
            let proxy = Login1ManagerProxy::builder(&conn)
                .destination(self.service_name.as_str())
                .path("/org/freedesktop/login1")
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| e.to_string())?;

            match tokio::time::timeout(UPSTREAM_TIMEOUT, proxy.can_reboot()).await {
                Ok(Ok(val)) => Ok(val.trim() == "yes"),
                Ok(Err(err)) => {
                    warn!("logind CanReboot call failed: {err}");
                    Ok(false)
                }
                Err(_) => {
                    warn!("logind CanReboot timed out after 2s");
                    Ok(false)
                }
            }
        })
    }

    fn can_power_off(&self) -> BoxFuture<'_, Result<bool, String>> {
        Box::pin(async move {
            let conn = {
                let lock = self.conn.read().await;
                match lock.as_ref() {
                    Some(c) => c.clone(),
                    None => return Ok(false),
                }
            };
            let proxy = Login1ManagerProxy::builder(&conn)
                .destination(self.service_name.as_str())
                .path("/org/freedesktop/login1")
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| e.to_string())?;

            match tokio::time::timeout(UPSTREAM_TIMEOUT, proxy.can_power_off()).await {
                Ok(Ok(val)) => Ok(val.trim() == "yes"),
                Ok(Err(err)) => {
                    warn!("logind CanPowerOff call failed: {err}");
                    Ok(false)
                }
                Err(_) => {
                    warn!("logind CanPowerOff timed out after 2s");
                    Ok(false)
                }
            }
        })
    }

    fn can_suspend(&self) -> BoxFuture<'_, Result<bool, String>> {
        Box::pin(async move {
            let conn = {
                let lock = self.conn.read().await;
                match lock.as_ref() {
                    Some(c) => c.clone(),
                    None => return Ok(false),
                }
            };
            let proxy = Login1ManagerProxy::builder(&conn)
                .destination(self.service_name.as_str())
                .path("/org/freedesktop/login1")
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| e.to_string())?;

            match tokio::time::timeout(UPSTREAM_TIMEOUT, proxy.can_suspend()).await {
                Ok(Ok(val)) => Ok(val.trim() == "yes"),
                Ok(Err(err)) => {
                    warn!("logind CanSuspend call failed: {err}");
                    Ok(false)
                }
                Err(_) => {
                    warn!("logind CanSuspend timed out after 2s");
                    Ok(false)
                }
            }
        })
    }

    fn reboot(&self, interactive: bool) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let conn = {
                let lock = self.conn.read().await;
                match lock.as_ref() {
                    Some(c) => c.clone(),
                    None => {
                        return Err("System power management service is unavailable".to_string())
                    }
                }
            };
            let proxy = Login1ManagerProxy::builder(&conn)
                .destination(self.service_name.as_str())
                .path("/org/freedesktop/login1")
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| e.to_string())?;

            match tokio::time::timeout(UPSTREAM_TIMEOUT, proxy.reboot(interactive)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(err.to_string()),
                Err(_) => {
                    Err("Action timed out: system service did not respond in 2 seconds".to_string())
                }
            }
        })
    }

    fn power_off(&self, interactive: bool) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let conn = {
                let lock = self.conn.read().await;
                match lock.as_ref() {
                    Some(c) => c.clone(),
                    None => {
                        return Err("System power management service is unavailable".to_string())
                    }
                }
            };
            let proxy = Login1ManagerProxy::builder(&conn)
                .destination(self.service_name.as_str())
                .path("/org/freedesktop/login1")
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| e.to_string())?;

            match tokio::time::timeout(UPSTREAM_TIMEOUT, proxy.power_off(interactive)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(err.to_string()),
                Err(_) => {
                    Err("Action timed out: system service did not respond in 2 seconds".to_string())
                }
            }
        })
    }

    fn suspend(&self, interactive: bool) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let conn = {
                let lock = self.conn.read().await;
                match lock.as_ref() {
                    Some(c) => c.clone(),
                    None => {
                        return Err("System power management service is unavailable".to_string())
                    }
                }
            };
            let proxy = Login1ManagerProxy::builder(&conn)
                .destination(self.service_name.as_str())
                .path("/org/freedesktop/login1")
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| e.to_string())?;

            match tokio::time::timeout(UPSTREAM_TIMEOUT, proxy.suspend(interactive)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(err.to_string()),
                Err(_) => {
                    Err("Action timed out: system service did not respond in 2 seconds".to_string())
                }
            }
        })
    }
}

/// Production implementation of `SystemdClient` stopping `kynex-session.target`.
#[derive(Clone, Debug)]
pub struct ZbusSystemdClient {
    conn: Arc<tokio::sync::RwLock<Option<Connection>>>,
}

impl ZbusSystemdClient {
    /// Creates a new `ZbusSystemdClient` with an optional connection.
    #[must_use]
    pub fn new(conn: Option<Connection>) -> Self {
        Self {
            conn: Arc::new(tokio::sync::RwLock::new(conn)),
        }
    }

    /// Sets or updates the D-Bus connection.
    pub async fn set_connection(&self, conn: Connection) {
        let mut lock = self.conn.write().await;
        *lock = Some(conn);
    }
}

impl SystemdClient for ZbusSystemdClient {
    fn stop_session_target(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let maybe_conn = {
                let lock = self.conn.read().await;
                lock.clone()
            };
            let Some(conn) = maybe_conn else {
                return Err("System manager service is unavailable".to_string());
            };

            let proxy = Systemd1ManagerProxy::builder(&conn)
                .destination("org.freedesktop.systemd1")
                .path("/org/freedesktop/systemd1")
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| e.to_string())?;

            match tokio::time::timeout(
                UPSTREAM_TIMEOUT,
                proxy.stop_unit("kynex-session.target", "replace"),
            )
            .await
            {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(err)) => Err(err.to_string()),
                Err(_) => {
                    Err("Action timed out: system manager did not respond in 2 seconds".to_string())
                }
            }
        })
    }
}

/// D-Bus server implementing `org.kynexos.Daemon.Session1`.
#[derive(Clone, Debug)]
pub struct SessionServer {
    logind: Arc<dyn LogindClient>,
    systemd: Arc<dyn SystemdClient>,
    rate_limiter: Arc<SessionRateLimiter>,
    emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
    can_logout: Arc<AtomicBool>,
    can_restart: Arc<AtomicBool>,
    can_power_off: Arc<AtomicBool>,
    can_suspend: Arc<AtomicBool>,
}

impl SessionServer {
    /// Creates a new `SessionServer`.
    #[must_use]
    pub fn new(
        logind: Arc<dyn LogindClient>,
        systemd: Arc<dyn SystemdClient>,
        rate_limiter: Arc<SessionRateLimiter>,
    ) -> Self {
        Self {
            logind,
            systemd,
            rate_limiter,
            emitter: Arc::new(Mutex::new(None)),
            can_logout: Arc::new(AtomicBool::new(true)),
            can_restart: Arc::new(AtomicBool::new(false)),
            can_power_off: Arc::new(AtomicBool::new(false)),
            can_suspend: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Sets the D-Bus signal emitter.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        let mut lock = self.emitter.lock().await;
        *lock = Some(emitter);
    }

    /// Queries upstream capabilities and updates cached properties.
    pub async fn refresh_capabilities(&self) {
        let can_r = self.logind.can_reboot().await.unwrap_or(false);
        let can_p = self.logind.can_power_off().await.unwrap_or(false);
        let can_s = self.logind.can_suspend().await.unwrap_or(false);

        self.can_restart.store(can_r, Ordering::SeqCst);
        self.can_power_off.store(can_p, Ordering::SeqCst);
        self.can_suspend.store(can_s, Ordering::SeqCst);
    }

    async fn emit_failure(&self, kind: &str, reason: &str) {
        if let Some(ref em) = *self.emitter.lock().await {
            let _ = Self::action_failed(em, kind, reason).await;
        }
    }
}

#[zbus::interface(name = "org.kynexos.Daemon.Session1")]
impl SessionServer {
    /// Whether session logout is supported.
    #[zbus(property)]
    fn can_logout(&self) -> bool {
        self.can_logout.load(Ordering::SeqCst)
    }

    /// Whether machine reboot/restart is authorized.
    #[zbus(property)]
    fn can_restart(&self) -> bool {
        self.can_restart.load(Ordering::SeqCst)
    }

    /// Whether machine power off / shut down is authorized.
    #[zbus(property)]
    fn can_power_off(&self) -> bool {
        self.can_power_off.load(Ordering::SeqCst)
    }

    /// Whether machine suspend is authorized.
    #[zbus(property)]
    fn can_suspend(&self) -> bool {
        self.can_suspend.load(Ordering::SeqCst)
    }

    /// Orders session logout by cleanly stopping `kynex-session.target`.
    async fn logout(&self) -> zbus::fdo::Result<()> {
        if !self.rate_limiter.check_and_record().await {
            let reason = sanitize_error_text("Rate limit exceeded");
            self.emit_failure("logout", &reason).await;
            return Err(zbus::fdo::Error::Failed(reason));
        }

        // Hard invariant: Log only action kind, NEVER arguments
        info!(kind = "logout", "Session power action requested");

        if let Err(err) = self.systemd.stop_session_target().await {
            let reason = sanitize_error_text(&err);
            self.emit_failure("logout", &reason).await;
            return Err(zbus::fdo::Error::Failed(reason));
        }

        Ok(())
    }

    /// Orders machine restart via logind `Reboot(interactive=false)`.
    async fn restart(&self) -> zbus::fdo::Result<()> {
        if !self.rate_limiter.check_and_record().await {
            let reason = sanitize_error_text("Rate limit exceeded");
            self.emit_failure("restart", &reason).await;
            return Err(zbus::fdo::Error::Failed(reason));
        }

        info!(kind = "restart", "Session power action requested");

        // Hard invariant: interactive = false
        if let Err(err) = self.logind.reboot(false).await {
            let reason = sanitize_error_text(&err);
            self.emit_failure("restart", &reason).await;
            return Err(zbus::fdo::Error::Failed(reason));
        }

        Ok(())
    }

    /// Orders machine shutdown via logind `PowerOff(interactive=false)`.
    async fn power_off(&self) -> zbus::fdo::Result<()> {
        if !self.rate_limiter.check_and_record().await {
            let reason = sanitize_error_text("Rate limit exceeded");
            self.emit_failure("poweroff", &reason).await;
            return Err(zbus::fdo::Error::Failed(reason));
        }

        info!(kind = "poweroff", "Session power action requested");

        if let Err(err) = self.logind.power_off(false).await {
            let reason = sanitize_error_text(&err);
            self.emit_failure("poweroff", &reason).await;
            return Err(zbus::fdo::Error::Failed(reason));
        }

        Ok(())
    }

    /// Orders machine suspend via logind `Suspend(interactive=false)`.
    async fn suspend(&self) -> zbus::fdo::Result<()> {
        if !self.rate_limiter.check_and_record().await {
            let reason = sanitize_error_text("Rate limit exceeded");
            self.emit_failure("suspend", &reason).await;
            return Err(zbus::fdo::Error::Failed(reason));
        }

        info!(kind = "suspend", "Session power action requested");

        if let Err(err) = self.logind.suspend(false).await {
            let reason = sanitize_error_text(&err);
            self.emit_failure("suspend", &reason).await;
            return Err(zbus::fdo::Error::Failed(reason));
        }

        Ok(())
    }

    /// Signal emitted when a session or power action fails.
    #[zbus(signal)]
    pub async fn action_failed(
        emitter: &SignalEmitter<'_>,
        kind: &str,
        reason: &str,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitization_removes_internal_paths_and_addresses() {
        let raw = "Failed to connect to socket unix:path=/run/user/1000/bus at 0x7ffd98e2: OperationInhibited";
        let clean = sanitize_error_text(raw);
        assert_eq!(clean, "An application is preventing this action");

        let raw_denied =
            "org.freedesktop.DBus.Error.AccessDenied: Interactive authentication required";
        let clean_denied = sanitize_error_text(raw_denied);
        assert_eq!(
            clean_denied,
            "Action refused: permission denied by system policy"
        );

        let raw_timeout = "DBus call failed: deadline has elapsed";
        let clean_timeout = sanitize_error_text(raw_timeout);
        assert_eq!(
            clean_timeout,
            "Action timed out: system service did not respond in 2 seconds"
        );

        let raw_service =
            "The name org.freedesktop.login1 was not provided by any .service files (not found)";
        let clean_service = sanitize_error_text(raw_service);
        assert_eq!(clean_service, "System service is unavailable");

        let raw_path = "Cannot write to /home/secret/credentials.txt at address 0xdeadbeef";
        let clean_path = sanitize_error_text(raw_path);
        assert!(!clean_path.contains("/home/secret"));
        assert!(!clean_path.contains("0xdeadbeef"));
        assert!(clean_path.contains("[path]"));
        assert!(clean_path.contains("[addr]"));
    }

    #[tokio::test]
    async fn test_rate_limiter_holds_burst() {
        let limiter = SessionRateLimiter::new(Duration::from_millis(50));
        assert!(limiter.check_and_record().await);
        // Immediate second call fails
        assert!(!limiter.check_and_record().await);
        // Wait for cooldown
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(limiter.check_and_record().await);

        let default_limiter = SessionRateLimiter::new_default();
        assert_eq!(default_limiter.window, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_zbus_logind_client_unconnected() {
        let client = ZbusLogindClient::new(None);
        assert!(!client.can_reboot().await.unwrap());
        assert!(!client.can_power_off().await.unwrap());
        assert!(!client.can_suspend().await.unwrap());

        let res_reboot = client.reboot(false).await;
        assert!(res_reboot.is_err());
        assert!(res_reboot.unwrap_err().contains("unavailable"));

        let res_power_off = client.power_off(false).await;
        assert!(res_power_off.is_err());
        assert!(res_power_off.unwrap_err().contains("unavailable"));

        let res_suspend = client.suspend(false).await;
        assert!(res_suspend.is_err());
        assert!(res_suspend.unwrap_err().contains("unavailable"));
    }

    #[tokio::test]
    async fn test_zbus_systemd_client_unconnected() {
        let client = ZbusSystemdClient::new(None);
        let res = client.stop_session_target().await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unavailable"));
    }

    #[test]
    fn test_sanitization_edge_cases() {
        assert_eq!(sanitize_error_text(""), "Action failed");
        assert_eq!(sanitize_error_text("/unix/path"), "Action failed");
        let long_input = "a".repeat(200);
        let sanitized = sanitize_error_text(&long_input);
        assert!(sanitized.len() <= 120);
    }
}
