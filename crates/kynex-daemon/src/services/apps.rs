//! Applications indexing and execution service implementing `org.kynexos.Daemon.Apps1`.
//!
//! Provides zero-polling, securely sandboxed application launching, prefix/word-start
//! ranked search, and debounced inotify watching over `$XDG_DATA_DIRS/applications`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use kynex_apps_index::{parse_exec, AppsIndex, AppsWatcher, DesktopEntry, ExecContext};
use kynex_ipc::apps::AppItem;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection};

/// Default maximum allowed launches per second from any single client.
pub const LAUNCH_RATE_LIMIT_PER_SEC: u32 = 5;

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

/// Shared internal state holding application index and launch rate limiters.
#[derive(Debug)]
pub struct AppsSharedState {
    /// In-memory application index.
    pub index: RwLock<AppsIndex>,
    /// Search application directories.
    pub dirs: Vec<PathBuf>,
    /// Rate limiters mapped by sender bus name.
    rate_limiters: Mutex<HashMap<String, RateLimitBucket>>,
    /// Active desktop environment.
    pub current_desktop: String,
}

impl AppsSharedState {
    /// Creates a new `AppsSharedState` scanning specified directories.
    #[must_use]
    pub fn new(dirs: Vec<PathBuf>, current_desktop: impl Into<String>) -> Self {
        let desktop = current_desktop.into();
        let index =
            AppsIndex::load_from_dirs(&dirs, &desktop).unwrap_or_else(|_| AppsIndex::new(&desktop));
        Self {
            index: RwLock::new(index),
            dirs,
            rate_limiters: Mutex::new(HashMap::new()),
            current_desktop: desktop,
        }
    }

    /// Reloads the application index from disk.
    pub async fn reload(&self) {
        if let Ok(new_index) = AppsIndex::load_from_dirs(&self.dirs, &self.current_desktop) {
            let mut lock = self.index.write().await;
            *lock = new_index;
        }
    }

    /// Verifies caller UID matches daemon session UID and enforces launch rate limiting.
    ///
    /// # Errors
    ///
    /// Returns `zbus::fdo::Error::AccessDenied` if caller UID does not match daemon UID,
    /// or `zbus::fdo::Error::LimitsExceeded` if launch rate limit is exceeded.
    pub async fn check_caller(&self, sender: &str, uid: u32) -> Result<(), zbus::fdo::Error> {
        let daemon_uid = rustix::process::getuid().as_raw();
        if uid != daemon_uid {
            warn!("Rejected unauthorized Apps1 caller '{sender}' with UID {uid} != daemon UID {daemon_uid}");
            return Err(zbus::fdo::Error::AccessDenied(
                "Unauthorized caller UID".into(),
            ));
        }

        let now = Instant::now();
        let mut buckets = self.rate_limiters.lock().await;
        let bucket = buckets
            .entry(sender.to_string())
            .or_insert_with(|| RateLimitBucket::new(now));

        if !bucket.check_and_inc(now, LAUNCH_RATE_LIMIT_PER_SEC) {
            warn!("Launch rate limit exceeded for sender '{sender}' (> {LAUNCH_RATE_LIMIT_PER_SEC} calls/s)");
            return Err(zbus::fdo::Error::LimitsExceeded(format!(
                "Launch rate limit exceeded (max {LAUNCH_RATE_LIMIT_PER_SEC} calls/s)"
            )));
        }

        Ok(())
    }
}

/// D-Bus object implementing `org.kynexos.Daemon.Apps1`.
#[derive(Debug, Clone)]
pub struct AppsServer {
    shared: Arc<AppsSharedState>,
}

impl AppsServer {
    /// Creates a new `AppsServer`.
    #[must_use]
    pub fn new(shared: Arc<AppsSharedState>) -> Self {
        Self { shared }
    }

    /// Converts a `DesktopEntry` to a D-Bus `AppItem`.
    #[must_use]
    pub fn entry_to_item(entry: &DesktopEntry, locale: Option<&str>) -> AppItem {
        AppItem {
            id: entry.id.clone(),
            name: entry.name.resolve(locale).to_string(),
            generic_name: entry.generic_name.resolve(locale).to_string(),
            comment: entry.comment.resolve(locale).to_string(),
            icon_name: entry.icon.clone(),
            categories: entry.categories.clone(),
        }
    }
}

#[interface(name = "org.kynexos.Daemon.Apps1")]
impl AppsServer {
    /// Signal emitted when applications index changes.
    #[zbus(signal)]
    pub async fn changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Returns the full list of available applications.
    async fn get_apps(&self) -> zbus::fdo::Result<Vec<AppItem>> {
        let locale = std::env::var("LC_MESSAGES")
            .or_else(|_| std::env::var("LANG"))
            .ok();
        let index = self.shared.index.read().await;
        let items = index
            .entries()
            .iter()
            .map(|e| Self::entry_to_item(e, locale.as_deref()))
            .collect();
        Ok(items)
    }

    /// Searches and ranks applications matching `query` up to `limit` entries.
    async fn search(&self, query: String, limit: u32) -> zbus::fdo::Result<Vec<AppItem>> {
        if query.chars().count() > 128 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Query length exceeds 128 characters".into(),
            ));
        }

        let cap = if limit == 0 {
            20
        } else {
            (limit as usize).min(50)
        };
        let locale = std::env::var("LC_MESSAGES")
            .or_else(|_| std::env::var("LANG"))
            .ok();

        let index = self.shared.index.read().await;
        let matches = index.search(&query, cap, locale.as_deref());
        let items = matches
            .into_iter()
            .map(|e| Self::entry_to_item(e, locale.as_deref()))
            .collect();

        Ok(items)
    }

    /// Spawns the application binary associated with desktop entry `id`.
    async fn launch(
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

        self.shared.check_caller(sender.as_str(), uid).await?;

        let entry = {
            let index = self.shared.index.read().await;
            index.get(&id).cloned().ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs(format!("Application ID '{id}' not found"))
            })?
        };

        if entry.terminal {
            return Err(zbus::fdo::Error::Failed(
                "Terminal applications are unsupported yet".into(),
            ));
        }

        let locale = std::env::var("LC_MESSAGES")
            .or_else(|_| std::env::var("LANG"))
            .ok();
        let app_name = entry.name.resolve(locale.as_deref());

        let ctx = ExecContext {
            name: app_name,
            icon: &entry.icon,
            file_path: &entry.path,
            files: &[],
            urls: &[],
        };

        let argv = parse_exec(&entry.exec_raw, &ctx)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to parse Exec: {e}")))?;

        if argv.is_empty() {
            return Err(zbus::fdo::Error::Failed(
                "Empty executable argument vector".into(),
            ));
        }

        // Log ONLY the id, NEVER arguments
        info!("Apps1: launching application ID '{id}'");

        let env_allowlist = build_clean_environment();
        spawn_detached(&argv, &env_allowlist)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to spawn process: {e}")))?;

        Ok(())
    }
}

/// Builds sanitized environment vector based on allowlist.
///
/// Allowed variables: `HOME`, `PATH`, `XDG_*`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `LANG`, `LC_*`, `DISPLAY`.
#[must_use]
pub fn build_clean_environment() -> Vec<(String, String)> {
    let mut clean_env = Vec::new();
    for (key, val) in std::env::vars() {
        if key == "HOME"
            || key == "PATH"
            || key.starts_with("XDG_")
            || key == "WAYLAND_DISPLAY"
            || key == "DBUS_SESSION_BUS_ADDRESS"
            || key == "LANG"
            || key.starts_with("LC_")
            || key == "DISPLAY"
        {
            clean_env.push((key, val));
        }
    }
    clean_env
}

/// Spawns an application completely detached from the daemon with a clean environment.
///
/// Desktop Entry Specification Section "The Exec key": Never passes through a shell.
/// Uses `process_group(0)` to detach process group, null stdin/stdout/stderr to prevent fd leaks,
/// and `env_clear()` with allowlist to prevent environment leakage.
/// When the daemon restarts, detached processes are reparented to PID 1 / user manager
/// and automatically reaped upon exit, preventing zombies.
fn spawn_detached(argv: &[String], env_vars: &[(String, String)]) -> Result<(), String> {
    if argv.is_empty() {
        return Err("argv is empty".to_string());
    }

    let mut cmd = Command::new(&argv[0]);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.env_clear();
    cmd.envs(env_vars.iter().cloned());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn()
        .map_err(|e| format!("Direct spawn failed: {e}"))?;

    Ok(())
}

/// Starts watching applications directories and links to `AppsServer`.
pub fn setup_apps_watcher(
    shared: Arc<AppsSharedState>,
    emitter: SignalEmitter<'static>,
) -> Option<AppsWatcher> {
    let dirs = shared.dirs.clone();

    AppsWatcher::new(&dirs, move || {
        let shared = Arc::clone(&shared);
        let emitter = emitter.clone();
        tokio::spawn(async move {
            shared.reload().await;
            let _ = AppsServer::changed(&emitter).await;
        });
    })
    .ok()
}
