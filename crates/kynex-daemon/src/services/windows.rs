//! Window tracking and `KWin` feeder service implementing `WindowsFeed1` and `Windows1`.
//!
//! Enforces caller authentication via `SenderPolicy` to prevent spoofing from untrusted
//! session bus clients, applies strict rate limiting (200 calls/s) per sender, and
//! dispatches atomic updates to shell clients with zero polling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use kynex_ipc::windows::WindowItem;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::names::BusName;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection};

use crate::window_state::{UpsertResult, WindowState};

/// Default maximum allowed feeder calls per second from any single sender.
pub const FEED_RATE_LIMIT_PER_SEC: u32 = 200;

/// Default maximum allowed window control actions per second from any single client.
pub const ACTION_RATE_LIMIT_PER_SEC: u32 = 20;

/// Actions performable on tracked windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    /// Focus and raise window.
    Activate,
    /// Minimize window.
    Minimize,
    /// Restore window from minimized state.
    Restore,
    /// Close window.
    Close,
}

/// Abstract executor for performing actions on `KWin` windows.
pub trait KWinActionExecutor: Send + Sync + std::fmt::Debug {
    /// Executes the specified window action for the validated window ID.
    fn execute_action(
        &self,
        id: &str,
        action: WindowAction,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
}

/// No-op action executor for testing or when compositor is absent.
#[derive(Debug, Clone, Default)]
pub struct NoOpActionExecutor;

impl KWinActionExecutor for NoOpActionExecutor {
    fn execute_action(
        &self,
        _id: &str,
        _action: WindowAction,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

/// Production executor synthesizing short-lived scripts and invoking them via `org.kde.kwin.Scripting`.
#[derive(Debug, Clone)]
pub struct KWinScriptActionExecutor {
    conn: Connection,
}

impl KWinScriptActionExecutor {
    /// Creates a new `KWinScriptActionExecutor` using the provided D-Bus connection.
    #[must_use]
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

static ACTION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Synthesizes the JavaScript script for the specified action and window ID from constant templates.
#[must_use]
pub fn generate_action_script(id: &str, action: WindowAction) -> String {
    match action {
        WindowAction::Activate => format!(
            r#"var ws = workspace.windowList();
for (var i = 0; i < ws.length; ++i) {{
    if (String(ws[i].internalId) === "{id}") {{
        if (ws[i].minimized) {{ ws[i].minimized = false; }}
        workspace.activeWindow = ws[i];
        break;
    }}
}}"#
        ),
        WindowAction::Minimize => format!(
            r#"var ws = workspace.windowList();
for (var i = 0; i < ws.length; ++i) {{
    if (String(ws[i].internalId) === "{id}") {{
        ws[i].minimized = true;
        break;
    }}
}}"#
        ),
        WindowAction::Restore => format!(
            r#"var ws = workspace.windowList();
for (var i = 0; i < ws.length; ++i) {{
    if (String(ws[i].internalId) === "{id}") {{
        ws[i].minimized = false;
        workspace.activeWindow = ws[i];
        break;
    }}
}}"#
        ),
        WindowAction::Close => format!(
            r#"var ws = workspace.windowList();
for (var i = 0; i < ws.length; ++i) {{
    if (String(ws[i].internalId) === "{id}") {{
        ws[i].closeWindow();
        break;
    }}
}}"#
        ),
    }
}

/// Creates a temporary directory with 0700 permissions and script file with 0600 permissions.
/// Returns `(tmp_dir, script_path, plugin_name)`.
///
/// # Errors
/// Returns `Err` if directory creation or file writing fails.
pub fn create_action_temp_file(
    id: &str,
    action: WindowAction,
) -> Result<(PathBuf, PathBuf, String), String> {
    use std::os::unix::fs::PermissionsExt;
    let counter = ACTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let rand_suffix = format!("{now_nanos:x}_{counter:x}_{}", std::process::id());
    let tmp_dir = std::env::temp_dir().join(format!("kynex-act-{rand_suffix}"));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&tmp_dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;

    let plugin_name = format!("kynex_act_{rand_suffix}");
    let script_path = tmp_dir.join(format!("{plugin_name}.js"));
    let script_content = generate_action_script(id, action);

    std::fs::write(&script_path, &script_content).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;

    Ok((tmp_dir, script_path, plugin_name))
}

impl KWinActionExecutor for KWinScriptActionExecutor {
    fn execute_action(
        &self,
        id: &str,
        action: WindowAction,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        let id = id.to_string();
        Box::pin(async move {
            // Verify KWin is running
            let dbus = DBusProxy::new(&self.conn)
                .await
                .map_err(|e| e.to_string())?;
            let _kwin_owner = dbus
                .get_name_owner(
                    BusName::from_static_str("org.kde.KWin").map_err(|e| e.to_string())?,
                )
                .await
                .map_err(|e| format!("KWin window manager service is unavailable: {e}"))?;

            let (tmp_dir, script_path, plugin_name) = create_action_temp_file(&id, action)?;
            let script_path_str = script_path
                .to_str()
                .ok_or_else(|| "Invalid UTF-8 script path".to_string())?;

            // Load script into KWin
            let load_res = self
                .conn
                .call_method(
                    Some("org.kde.KWin"),
                    "/Scripting",
                    Some("org.kde.kwin.Scripting"),
                    "loadScript",
                    &(script_path_str, plugin_name.as_str()),
                )
                .await;

            let script_id: i32 = match load_res {
                Ok(msg) => msg
                    .body()
                    .deserialize()
                    .map_err(|e| format!("Failed to parse script ID: {e}"))?,
                Err(e) => {
                    let _ = std::fs::remove_file(&script_path);
                    let _ = std::fs::remove_dir(&tmp_dir);
                    return Err(format!("loadScript failed on KWin: {e}"));
                }
            };

            if script_id < 0 {
                let _ = std::fs::remove_file(&script_path);
                let _ = std::fs::remove_dir(&tmp_dir);
                return Err(format!("KWin returned error script ID: {script_id}"));
            }

            // Run script
            let script_obj_path = format!("/Scripting/Script{script_id}");
            let run_res = self
                .conn
                .call_method(
                    Some("org.kde.KWin"),
                    script_obj_path.as_str(),
                    Some("org.kde.kwin.Script"),
                    "run",
                    &(),
                )
                .await;

            // Unload script
            let _ = self
                .conn
                .call_method(
                    Some("org.kde.KWin"),
                    "/Scripting",
                    Some("org.kde.kwin.Scripting"),
                    "unloadScript",
                    &(plugin_name.as_str(),),
                )
                .await;

            // Clean up temporary disk artifacts immediately
            let _ = std::fs::remove_file(&script_path);
            let _ = std::fs::remove_dir(&tmp_dir);

            run_res
                .map(|_| ())
                .map_err(|e| format!("Script run failed on KWin: {e}"))
        })
    }
}

/// Mock action executor that records invocations and simulates responses.
#[derive(Debug, Default)]
pub struct MockActionExecutor {
    /// Invocations recorded as (window ID, action).
    pub calls: Mutex<Vec<(String, WindowAction)>>,
    /// If set to Some, simulates a `KWin` failure/unavailability.
    pub fail_with: Mutex<Option<String>>,
}

impl MockActionExecutor {
    /// Creates a new `MockActionExecutor`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an error message to return on subsequent executions.
    pub async fn set_fail_with(&self, err: Option<String>) {
        *self.fail_with.lock().await = err;
    }

    /// Returns recorded action calls.
    pub async fn get_calls(&self) -> Vec<(String, WindowAction)> {
        self.calls.lock().await.clone()
    }
}

impl KWinActionExecutor for MockActionExecutor {
    fn execute_action(
        &self,
        id: &str,
        action: WindowAction,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        let id = id.to_string();
        Box::pin(async move {
            if let Some(ref err) = *self.fail_with.lock().await {
                return Err(err.clone());
            }
            self.calls.lock().await.push((id, action));
            Ok(())
        })
    }
}

/// Result of inspecting `/proc/<pid>/exe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExeInspectResult {
    /// Canonical executable path was successfully read.
    Path(PathBuf),
    /// Access was denied (e.g. kernel non-dumpable restriction due to file capabilities).
    PermissionDenied,
    /// Process has terminated or vanished (`/proc/<pid>/exe` not found).
    NotFound,
    /// Any other inspection error.
    Other(String),
}

/// Authentication outcome from `evaluate_feeder_auth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeederAuthOutcome {
    /// Authenticated via exact canonical executable match.
    ExactExecutable,
    /// Authenticated via session UID and org.kde.KWin name ownership due to capability restriction.
    CapabilityRestricted,
}

/// Error returned by `evaluate_feeder_auth` when authentication fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeederAuthError {
    /// Caller UID does not match daemon session UID.
    UidMismatch {
        /// Caller process UID.
        caller: u32,
        /// Daemon process UID.
        daemon: u32,
    },
    /// Caller unique bus name does not match the current owner of `org.kde.KWin`.
    NotKWinOwner {
        /// Caller unique bus name.
        sender: String,
        /// Current owner of `org.kde.KWin` if any.
        current_owner: Option<String>,
    },
    /// Caller executable does not match the canonical path.
    UnauthorizedExecutable {
        /// Observed executable path.
        path: String,
        /// Caller PID.
        pid: u32,
        /// Expected canonical executable path.
        expected: String,
    },
    /// Caller process exited before `/proc/<pid>/exe` could be inspected.
    CallerExited(u32),
    /// Inspection of `/proc/<pid>/exe` failed with an unexpected error.
    InspectionFailed {
        /// Caller PID.
        pid: u32,
        /// Error description.
        error: String,
    },
}

impl std::fmt::Display for FeederAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UidMismatch { caller, daemon } => {
                write!(
                    f,
                    "UID mismatch: caller UID {caller} does not match daemon UID {daemon}"
                )
            }
            Self::NotKWinOwner {
                sender,
                current_owner,
            } => {
                write!(
                    f,
                    "Caller '{sender}' does not own 'org.kde.KWin' (current owner: {current_owner:?})"
                )
            }
            Self::UnauthorizedExecutable {
                path,
                pid,
                expected,
            } => {
                write!(
                    f,
                    "Unauthorized caller executable '{path}' for PID {pid}: expected '{expected}'"
                )
            }
            Self::CallerExited(pid) => {
                write!(
                    f,
                    "Caller PID {pid} terminated before credentials could be inspected"
                )
            }
            Self::InspectionFailed { pid, error } => {
                write!(
                    f,
                    "Failed to inspect caller executable for PID {pid}: {error}"
                )
            }
        }
    }
}

impl std::error::Error for FeederAuthError {}

/// Pure, fail-closed feeder authentication decision function.
///
/// Layered evaluation:
/// 1) Caller UID must match daemon session UID (`GetConnectionCredentials`).
/// 2) Caller unique bus name must match the current owner of `org.kde.KWin`.
/// 3) Caller executable must match `expected_exe` exactly if readable; if unreadable
///    due to file capabilities (`cap_sys_nice=ep`), rely on 1+2; any other error rejects.
///
/// # Errors
/// Returns `FeederAuthError` if any check fails.
pub fn evaluate_feeder_auth(
    caller_uid: u32,
    daemon_uid: u32,
    sender: &str,
    current_kwin_owner: Option<&str>,
    pid: u32,
    exe_result: &ExeInspectResult,
    expected_exe: &Path,
) -> Result<FeederAuthOutcome, FeederAuthError> {
    if caller_uid != daemon_uid {
        return Err(FeederAuthError::UidMismatch {
            caller: caller_uid,
            daemon: daemon_uid,
        });
    }

    if current_kwin_owner != Some(sender) {
        return Err(FeederAuthError::NotKWinOwner {
            sender: sender.to_string(),
            current_owner: current_kwin_owner.map(ToString::to_string),
        });
    }

    match exe_result {
        ExeInspectResult::Path(path) => {
            if path == expected_exe {
                Ok(FeederAuthOutcome::ExactExecutable)
            } else {
                Err(FeederAuthError::UnauthorizedExecutable {
                    path: path.display().to_string(),
                    pid,
                    expected: expected_exe.display().to_string(),
                })
            }
        }
        ExeInspectResult::PermissionDenied => Ok(FeederAuthOutcome::CapabilityRestricted),
        ExeInspectResult::NotFound => Err(FeederAuthError::CallerExited(pid)),
        ExeInspectResult::Other(err) => Err(FeederAuthError::InspectionFailed {
            pid,
            error: err.clone(),
        }),
    }
}

/// Policy interface defining sender authorization checks.
pub trait SenderPolicy: std::fmt::Debug + Send + Sync {
    /// Verifies whether the caller is authorized to feed window events.
    ///
    /// # Errors
    /// Returns `Err` with a diagnostic explanation if caller is unauthorized or inspection fails.
    fn verify_sender(
        &self,
        sender: &str,
        uid: u32,
        pid: u32,
        current_kwin_owner: Option<&str>,
    ) -> Result<(), String>;
}

/// Production sender policy: requires session UID match, `org.kde.KWin` name ownership,
/// and exact canonical executable `/usr/bin/kwin_wayland` (or capability-restricted fallback).
#[derive(Debug)]
pub struct KWinSenderPolicy {
    daemon_uid: u32,
    expected_exe: PathBuf,
    capability_logged: AtomicBool,
}

impl KWinSenderPolicy {
    /// Creates a new `KWinSenderPolicy` bound to the current process UID and `/usr/bin/kwin_wayland`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_expected_exe(PathBuf::from("/usr/bin/kwin_wayland"))
    }

    /// Creates a `KWinSenderPolicy` with an explicitly configured expected executable path.
    #[must_use]
    pub fn with_expected_exe(expected_exe: PathBuf) -> Self {
        let daemon_uid = rustix::process::getuid().as_raw();
        Self {
            daemon_uid,
            expected_exe,
            capability_logged: AtomicBool::new(false),
        }
    }

    /// Inspects `/proc/<pid>/exe` symlink.
    #[must_use]
    pub fn inspect_proc_exe(pid: u32) -> ExeInspectResult {
        let exe_link = format!("/proc/{pid}/exe");
        match std::fs::read_link(&exe_link) {
            Ok(path) => ExeInspectResult::Path(path),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                ExeInspectResult::PermissionDenied
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ExeInspectResult::NotFound,
            Err(e) => ExeInspectResult::Other(e.to_string()),
        }
    }
}

impl Default for KWinSenderPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl SenderPolicy for KWinSenderPolicy {
    fn verify_sender(
        &self,
        sender: &str,
        uid: u32,
        pid: u32,
        current_kwin_owner: Option<&str>,
    ) -> Result<(), String> {
        let exe_result = Self::inspect_proc_exe(pid);
        let outcome = evaluate_feeder_auth(
            uid,
            self.daemon_uid,
            sender,
            current_kwin_owner,
            pid,
            &exe_result,
            &self.expected_exe,
        )
        .map_err(|e| e.to_string())?;

        if outcome == FeederAuthOutcome::CapabilityRestricted
            && !self.capability_logged.swap(true, Ordering::Relaxed)
        {
            info!(
                "KWin feeder PID {pid} /proc/<pid>/exe is unreadable due to file capabilities (cap_sys_nice=ep); authenticated via session UID and org.kde.KWin name ownership"
            );
        }

        Ok(())
    }
}

/// Permissive policy for unit and integration testing.
#[derive(Debug, Clone, Default)]
pub struct AllowAllSenderPolicy;

impl SenderPolicy for AllowAllSenderPolicy {
    fn verify_sender(
        &self,
        _sender: &str,
        _uid: u32,
        _pid: u32,
        _current_kwin_owner: Option<&str>,
    ) -> Result<(), String> {
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
    /// Cached owner of `org.kde.KWin`.
    pub tracked_kwin_owner: RwLock<Option<String>>,
    /// Window action executor for `KWin`.
    pub action_executor: RwLock<Arc<dyn KWinActionExecutor>>,
    /// Rate limiters for window control actions.
    action_rate_limiters: Mutex<HashMap<String, RateLimitBucket>>,
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
            tracked_kwin_owner: RwLock::new(None),
            action_executor: RwLock::new(Arc::new(NoOpActionExecutor)),
            action_rate_limiters: Mutex::new(HashMap::new()),
        }
    }

    /// Sets the action executor used to dispatch window actions to `KWin`.
    pub async fn set_action_executor(&self, executor: Arc<dyn KWinActionExecutor>) {
        *self.action_executor.write().await = executor;
    }

    /// Verifies window action caller credentials and enforces action rate limiting.
    ///
    /// # Errors
    /// Returns `Err` with D-Bus access denied or limits exceeded errors.
    pub async fn check_action_caller(
        &self,
        sender: &str,
        uid: u32,
    ) -> Result<(), zbus::fdo::Error> {
        let daemon_uid = rustix::process::getuid().as_raw();
        if uid != daemon_uid {
            self.dropped_calls.fetch_add(1, Ordering::Relaxed);
            warn!(
                "Rejected unauthorized action caller '{sender}' with UID {uid} != daemon UID {daemon_uid}"
            );
            return Err(zbus::fdo::Error::AccessDenied(
                "Unauthorized caller UID".into(),
            ));
        }

        let now = Instant::now();
        let mut buckets = self.action_rate_limiters.lock().await;
        let bucket = buckets
            .entry(sender.to_string())
            .or_insert_with(|| RateLimitBucket::new(now));

        if !bucket.check_and_inc(now, ACTION_RATE_LIMIT_PER_SEC) {
            warn!(
                "Action rate limit exceeded for sender '{sender}' (> {ACTION_RATE_LIMIT_PER_SEC} calls/s)"
            );
            self.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return Err(zbus::fdo::Error::LimitsExceeded(format!(
                "Rate limit exceeded (max {ACTION_RATE_LIMIT_PER_SEC} calls/s)"
            )));
        }

        Ok(())
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
        current_kwin_owner: Option<&str>,
    ) -> Result<(), zbus::fdo::Error> {
        if let Err(err) = self
            .policy
            .verify_sender(sender, uid, pid, current_kwin_owner)
        {
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
    /// Handles execution of window actions (Activate, Minimize, Restore, Close).
    async fn handle_action(
        &self,
        hdr: Header<'_>,
        conn: &Connection,
        raw_id: &str,
        action: WindowAction,
    ) -> zbus::fdo::Result<()> {
        let sender = hdr
            .sender()
            .ok_or_else(|| zbus::fdo::Error::AccessDenied("Missing message sender".into()))?;

        let dbus = DBusProxy::new(conn).await?;
        let uid = dbus.get_connection_unix_user(sender.clone().into()).await?;

        self.shared
            .check_action_caller(sender.as_str(), uid)
            .await?;

        let exists = self.shared.state.read().await.has_window(raw_id);
        if !exists {
            self.shared.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Window ID '{raw_id}' does not exist in tracked state"
            )));
        }

        let executor = Arc::clone(&*self.shared.action_executor.read().await);
        executor.execute_action(raw_id, action).await.map_err(|e| {
            warn!("Failed to execute window action {action:?} on '{raw_id}': {e}");
            zbus::fdo::Error::Failed(e)
        })?;

        Ok(())
    }
}

#[interface(name = "org.kynexos.Daemon.Windows1")]
impl WindowsServer {
    /// Returns the complete array of currently tracked windows.
    async fn list(&self) -> Vec<WindowItem> {
        self.shared.state.read().await.list().to_vec()
    }

    /// Activates (focuses and raises) the window matching `id`.
    async fn activate(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        id: String,
    ) -> zbus::fdo::Result<()> {
        self.handle_action(hdr, conn, &id, WindowAction::Activate)
            .await
    }

    /// Minimizes the window matching `id`.
    async fn minimize(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        id: String,
    ) -> zbus::fdo::Result<()> {
        self.handle_action(hdr, conn, &id, WindowAction::Minimize)
            .await
    }

    /// Restores the window matching `id` from minimized state.
    async fn restore(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        id: String,
    ) -> zbus::fdo::Result<()> {
        self.handle_action(hdr, conn, &id, WindowAction::Restore)
            .await
    }

    /// Closes the window matching `id`.
    async fn close(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        id: String,
    ) -> zbus::fdo::Result<()> {
        self.handle_action(hdr, conn, &id, WindowAction::Close)
            .await
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

        // Early check for no-op before performing D-Bus roundtrips
        {
            let state = self.shared.state.read().await;
            if let Some(w) = state.list().iter().find(|w| w.id == id) {
                if w.title == title
                    && w.app_id == app_id
                    && w.minimized == minimized
                    && w.active == active
                {
                    return Ok(());
                }
            }
        }

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

        // Early check for no-op before performing D-Bus roundtrips
        {
            let state = self.shared.state.read().await;
            let current_active = state.active_id().unwrap_or("");
            if current_active == id {
                return Ok(());
            }
        }

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

    if let Ok(owner) = dbus
        .get_name_owner(BusName::from_static_str("org.kde.KWin").unwrap_or_else(|_| unreachable!()))
        .await
    {
        *server.shared.tracked_kwin_owner.write().await = Some(owner.to_string());
    }

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
            let new_owner_str = new_owner.as_ref().and_then(|o| {
                let s = o.as_str();
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            });
            *server.shared.tracked_kwin_owner.write().await = new_owner_str.clone();
            if new_owner_str.is_none() {
                info!("KWin left the bus; clearing tracked windows");
                server.clear(&emitter).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feeder_auth_exact_executable() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::Path(PathBuf::from("/usr/bin/kwin_wayland"));
        let res = evaluate_feeder_auth(1000, 1000, ":1.42", Some(":1.42"), 1234, &exe, expected);
        assert_eq!(res, Ok(FeederAuthOutcome::ExactExecutable));
    }

    #[test]
    fn test_feeder_auth_capability_restricted() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::PermissionDenied;
        let res = evaluate_feeder_auth(1000, 1000, ":1.42", Some(":1.42"), 1234, &exe, expected);
        assert_eq!(res, Ok(FeederAuthOutcome::CapabilityRestricted));
    }

    #[test]
    fn test_feeder_auth_rogue_temp_binary_rejected() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::Path(PathBuf::from("/home/saif/dev/tmp/kwin_wayland"));
        let res = evaluate_feeder_auth(1000, 1000, ":1.42", Some(":1.42"), 1234, &exe, expected);
        match res {
            Err(FeederAuthError::UnauthorizedExecutable {
                path,
                pid,
                expected: exp,
            }) => {
                assert_eq!(path, "/home/saif/dev/tmp/kwin_wayland");
                assert_eq!(pid, 1234);
                assert_eq!(exp, "/usr/bin/kwin_wayland");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_feeder_auth_not_kwin_owner_rejected() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::Path(PathBuf::from("/usr/bin/kwin_wayland"));
        let res = evaluate_feeder_auth(1000, 1000, ":1.99", Some(":1.42"), 1234, &exe, expected);
        match res {
            Err(FeederAuthError::NotKWinOwner {
                sender,
                current_owner,
            }) => {
                assert_eq!(sender, ":1.99");
                assert_eq!(current_owner, Some(":1.42".to_string()));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_feeder_auth_no_kwin_owner_rejected() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::Path(PathBuf::from("/usr/bin/kwin_wayland"));
        let res = evaluate_feeder_auth(1000, 1000, ":1.42", None, 1234, &exe, expected);
        match res {
            Err(FeederAuthError::NotKWinOwner {
                sender,
                current_owner,
            }) => {
                assert_eq!(sender, ":1.42");
                assert_eq!(current_owner, None);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_feeder_auth_rogue_takes_name_after_kwin_dies_rejected_by_exe() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::Path(PathBuf::from("/tmp/rogue_busctl"));
        let res = evaluate_feeder_auth(1000, 1000, ":1.88", Some(":1.88"), 5555, &exe, expected);
        assert!(matches!(
            res,
            Err(FeederAuthError::UnauthorizedExecutable { .. })
        ));
    }

    #[test]
    fn test_feeder_auth_caller_exited_rejected() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::NotFound;
        let res = evaluate_feeder_auth(1000, 1000, ":1.42", Some(":1.42"), 9999, &exe, expected);
        assert_eq!(res, Err(FeederAuthError::CallerExited(9999)));
    }

    #[test]
    fn test_feeder_auth_inspection_failed_rejected() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::Other("I/O device error".into());
        let res = evaluate_feeder_auth(1000, 1000, ":1.42", Some(":1.42"), 8888, &exe, expected);
        assert_eq!(
            res,
            Err(FeederAuthError::InspectionFailed {
                pid: 8888,
                error: "I/O device error".into()
            })
        );
    }

    #[test]
    fn test_feeder_auth_uid_mismatch_rejected() {
        let expected = Path::new("/usr/bin/kwin_wayland");
        let exe = ExeInspectResult::Path(PathBuf::from("/usr/bin/kwin_wayland"));
        let res = evaluate_feeder_auth(1001, 1000, ":1.42", Some(":1.42"), 1234, &exe, expected);
        assert_eq!(
            res,
            Err(FeederAuthError::UidMismatch {
                caller: 1001,
                daemon: 1000
            })
        );
    }

    #[test]
    fn test_feeder_auth_error_display_formatting() {
        let err1 = FeederAuthError::UidMismatch {
            caller: 1001,
            daemon: 1000,
        };
        assert!(err1.to_string().contains("UID mismatch"));

        let err2 = FeederAuthError::NotKWinOwner {
            sender: ":1.99".into(),
            current_owner: Some(":1.42".into()),
        };
        assert!(err2.to_string().contains("does not own 'org.kde.KWin'"));

        let err3 = FeederAuthError::UnauthorizedExecutable {
            path: "/tmp/fake".into(),
            pid: 123,
            expected: "/usr/bin/kwin_wayland".into(),
        };
        assert!(err3.to_string().contains("Unauthorized caller executable"));

        let err4 = FeederAuthError::CallerExited(456);
        assert!(err4.to_string().contains("terminated before"));

        let err5 = FeederAuthError::InspectionFailed {
            pid: 789,
            error: "bad read".into(),
        };
        assert!(err5.to_string().contains("Failed to inspect caller"));
    }

    #[test]
    fn test_kwin_sender_policy_self_inspection() {
        let process_id = std::process::id();
        let user_id = rustix::process::getuid().as_raw();
        let my_exe = match std::fs::read_link(format!("/proc/{process_id}/exe")) {
            Ok(p) => p,
            Err(_) => PathBuf::from("/proc/self/exe"),
        };

        // 1. Policy configured with our own test runner exe -> must accept when sender is owner
        let policy = KWinSenderPolicy::with_expected_exe(my_exe.clone());
        assert!(policy
            .verify_sender(":1.1", user_id, process_id, Some(":1.1"))
            .is_ok());

        // 2. Sender mismatch -> rejected
        assert!(policy
            .verify_sender(":1.2", user_id, process_id, Some(":1.1"))
            .is_err());

        // 3. Expected /usr/bin/kwin_wayland, but we are the test binary -> rejected
        let prod_policy = KWinSenderPolicy::new();
        assert!(prod_policy
            .verify_sender(":1.1", user_id, process_id, Some(":1.1"))
            .is_err());
    }

    #[test]
    fn test_generate_action_script() {
        let id = "{11223344-5566}";

        let act_script = generate_action_script(id, WindowAction::Activate);
        assert!(act_script.contains(r#"String(ws[i].internalId) === "{11223344-5566}""#));
        assert!(act_script.contains("workspace.activeWindow = ws[i];"));

        let min_script = generate_action_script(id, WindowAction::Minimize);
        assert!(min_script.contains(r#"String(ws[i].internalId) === "{11223344-5566}""#));
        assert!(min_script.contains("ws[i].minimized = true;"));

        let rest_script = generate_action_script(id, WindowAction::Restore);
        assert!(rest_script.contains(r#"String(ws[i].internalId) === "{11223344-5566}""#));
        assert!(rest_script.contains("ws[i].minimized = false;"));

        let close_script = generate_action_script(id, WindowAction::Close);
        assert!(close_script.contains(r#"String(ws[i].internalId) === "{11223344-5566}""#));
        assert!(close_script.contains("ws[i].closeWindow();"));
    }

    #[test]
    fn test_create_action_temp_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let id = "{aabbccdd-eeff}";
        let res = create_action_temp_file(id, WindowAction::Minimize);
        assert!(res.is_ok());
        let Ok((tmp_dir, script_path, plugin_name)) = res else {
            return;
        };

        assert!(tmp_dir.exists());
        assert!(script_path.exists());
        assert!(plugin_name.starts_with("kynex_act_"));

        let Ok(dir_meta) = std::fs::metadata(&tmp_dir) else {
            return;
        };
        assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);

        let Ok(file_meta) = std::fs::metadata(&script_path) else {
            return;
        };
        assert_eq!(file_meta.permissions().mode() & 0o777, 0o600);

        let Ok(content) = std::fs::read_to_string(&script_path) else {
            return;
        };
        assert!(content.contains(id));

        // Clean up
        let _ = std::fs::remove_file(&script_path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    #[tokio::test]
    async fn test_noop_and_mock_action_executors() {
        let noop = NoOpActionExecutor;
        assert!(noop
            .execute_action("id1", WindowAction::Activate)
            .await
            .is_ok());

        let mock = MockActionExecutor::new();
        assert!(mock
            .execute_action("id2", WindowAction::Close)
            .await
            .is_ok());
        assert_eq!(
            mock.get_calls().await,
            vec![("id2".to_string(), WindowAction::Close)]
        );

        mock.set_fail_with(Some("KWin crashed".into())).await;
        assert!(mock
            .execute_action("id2", WindowAction::Close)
            .await
            .is_err());
    }
}
