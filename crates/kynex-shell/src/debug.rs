//! D-Bus debug interface `org.kynexos.Shell.Debug1` and shared observability hub.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::Mutex;
use tracing::{info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::Connection;

use kynex_ipc::debug::{
    SHELL_DEBUG_INTERFACE, SHELL_DEBUG_PATH, SHELL_DEBUG_SUBPATH, SHELL_DESTINATION,
};
use kynex_ipc::windows::WindowItem;
use kynex_shell_core::debug::{
    ModelReadinessState, NotificationDebugState, SessionDebugState, ShellDebugState,
    TaskbarDebugState, TaskbarEntryDebugState, WindowDebugState,
};
use kynex_shell_core::taskbar::TaskbarCore;

/// Central thread-safe state hub for shell debugging and observability.
#[derive(Debug, Default)]
pub struct ShellDebugHub {
    daemon_connected: AtomicBool,
    models_ready: AtomicBool,
    last_resync_time: Mutex<Option<String>>,
    model_readiness: Mutex<ModelReadinessState>,
    taskbar_core: Mutex<TaskbarCore>,
    local_windows: Mutex<Vec<WindowItem>>,
    notification_ids: Mutex<Vec<u32>>,
    session_state: Mutex<SessionDebugState>,
}

static DEBUG_HUB: OnceLock<Arc<ShellDebugHub>> = OnceLock::new();

/// Returns the process-wide `ShellDebugHub` singleton.
#[must_use]
pub fn get_debug_hub() -> Arc<ShellDebugHub> {
    DEBUG_HUB
        .get_or_init(|| Arc::new(ShellDebugHub::default()))
        .clone()
}

impl ShellDebugHub {
    /// Sets whether the daemon is currently connected on the session bus.
    pub fn set_daemon_connected(&self, connected: bool) {
        self.daemon_connected.store(connected, Ordering::SeqCst);
    }

    /// Sets whether shell models are initialized and ready.
    pub fn set_models_ready(&self, ready: bool) {
        self.models_ready.store(ready, Ordering::SeqCst);
    }

    /// Returns whether shell models are ready.
    #[must_use]
    pub fn models_ready(&self) -> bool {
        self.models_ready.load(Ordering::SeqCst)
    }

    /// Sets the timestamp of the last successful resynchronization.
    pub async fn set_last_resync_time(&self, time: String) {
        *self.last_resync_time.lock().await = Some(time);
    }

    /// Updates individual sub-model readiness.
    pub async fn update_model_readiness<F: FnOnce(&mut ModelReadinessState)>(&self, f: F) {
        let mut lock = self.model_readiness.lock().await;
        f(&mut lock);
        let ready = lock.windows
            && lock.apps
            && lock.pins
            && lock.audio
            && lock.network
            && lock.power
            && lock.notifications
            && lock.tray
            && lock.session;
        self.models_ready.store(ready, Ordering::SeqCst);
    }

    /// Updates taskbar state and local windows list.
    pub async fn update_taskbar(&self, core: TaskbarCore, windows: Vec<WindowItem>) {
        *self.taskbar_core.lock().await = core;
        *self.local_windows.lock().await = windows;
    }

    /// Updates active notification IDs.
    pub async fn update_notifications(&self, ids: Vec<u32>) {
        *self.notification_ids.lock().await = ids;
    }

    /// Updates session and power debug state.
    pub async fn update_session(&self, state: SessionDebugState) {
        *self.session_state.lock().await = state;
    }

    /// Takes a sanitized snapshot of the current shell state.
    pub async fn snapshot(&self) -> ShellDebugState {
        let daemon_connected = self.daemon_connected.load(Ordering::SeqCst);
        let models_ready = self.models_ready.load(Ordering::SeqCst);
        let last_resync_time = self.last_resync_time.lock().await.clone();
        let models = self.model_readiness.lock().await.clone();
        let session = self.session_state.lock().await.clone();

        let (taskbar_debug, windows_debug) = {
            let core = self.taskbar_core.lock().await;
            let wins = self.local_windows.lock().await;

            let entries = core
                .entries()
                .iter()
                .map(|e| TaskbarEntryDebugState {
                    app_id: e.app_id.clone(),
                    is_pinned: e.is_pinned,
                    is_running: e.is_running,
                    is_active: e.is_active,
                    is_minimized: e.is_minimized,
                    window_count: e.window_count,
                    window_ids: e.window_ids.clone(),
                    letter_badge: e.letter_badge.clone(),
                })
                .collect::<Vec<_>>();

            let window_count = wins.len();
            let pinned_count = entries.iter().filter(|e| e.is_pinned).count();
            let entry_count = entries.len();

            let taskbar_state = TaskbarDebugState {
                entry_count,
                window_count,
                pinned_count,
                entries,
            };

            let windows_state = wins
                .iter()
                .map(|w| WindowDebugState {
                    id: w.id.clone(),
                    app_id: w.app_id.clone(),
                    minimized: w.minimized,
                    active: w.active,
                })
                .collect::<Vec<_>>();

            (taskbar_state, windows_state)
        };

        let notification_state = {
            let ids = self.notification_ids.lock().await;
            NotificationDebugState {
                active_count: ids.len(),
                notification_ids: ids.clone(),
            }
        };

        ShellDebugState {
            daemon_connected,
            models_ready,
            last_resync_time,
            models,
            taskbar: taskbar_debug,
            windows: windows_debug,
            notifications: notification_state,
            session,
        }
    }
}

/// D-Bus object implementing `org.kynexos.Shell.Debug1`.
#[derive(Clone)]
pub struct ShellDebugServer {
    hub: Arc<ShellDebugHub>,
}

impl ShellDebugServer {
    /// Creates a new `ShellDebugServer` wrapping the provided hub.
    #[must_use]
    pub fn new(hub: Arc<ShellDebugHub>) -> Self {
        Self { hub }
    }

    /// Helper to enforce same-UID caller check for Debug1 methods.
    async fn check_caller(&self, hdr: &Header<'_>, conn: &Connection) -> zbus::fdo::Result<()> {
        let sender = hdr
            .sender()
            .ok_or_else(|| zbus::fdo::Error::AccessDenied("Missing message sender".into()))?;

        let dbus = DBusProxy::new(conn).await?;
        let uid = dbus.get_connection_unix_user(sender.clone().into()).await?;
        let my_uid = rustix::process::getuid().as_raw();
        if uid != my_uid {
            warn!("Rejected unauthorized Debug1 caller '{sender}' with UID {uid} != shell UID {my_uid}");
            return Err(zbus::fdo::Error::AccessDenied(
                "Caller UID does not match".into(),
            ));
        }
        Ok(())
    }
}

#[zbus::interface(name = "org.kynexos.Shell.Debug1")]
impl ShellDebugServer {
    /// Dumps the current shell state as a sanitized JSON payload.
    ///
    /// Restricts access to the same UID.
    async fn dump_state(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<String> {
        self.check_caller(&hdr, conn).await?;

        let state = self.hub.snapshot().await;
        state
            .to_json()
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Dumps the current QML scene as sanitized JSON (strictly NO titles).
    ///
    /// Restricts access to the same UID.
    async fn dump_scene(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<String> {
        self.check_caller(&hdr, conn).await?;
        Ok(crate::shim::dump_active_scene())
    }

    /// Grabs a PNG of the panel window to `$XDG_RUNTIME_DIR/kynex/panel-<timestamp>.png`
    /// with mode 0600 and returns the path.
    ///
    /// Restricts access to the same UID.
    async fn grab_panel(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<String> {
        self.check_caller(&hdr, conn).await?;
        let path = crate::shim::grab_active_panel();
        if path.is_empty() {
            return Err(zbus::fdo::Error::Failed(
                "Failed to grab panel image".into(),
            ));
        }
        Ok(path)
    }

    /// Returns whether primary models are ready.
    #[zbus(property)]
    fn models_ready(&self) -> bool {
        self.hub.models_ready()
    }
}

/// Registers the debug service and requests the well-known name `org.kynexos.Shell`.
///
/// # Errors
/// Returns `Err` if object registration or name acquisition fails.
pub async fn register_shell_debug_service(conn: &Connection) -> zbus::Result<()> {
    let hub = get_debug_hub();
    let server = ShellDebugServer::new(hub);
    conn.object_server()
        .at(SHELL_DEBUG_PATH, server.clone())
        .await?;
    conn.object_server().at(SHELL_DEBUG_SUBPATH, server).await?;
    let _ = conn.request_name(SHELL_DESTINATION).await;
    info!("Registered {SHELL_DEBUG_INTERFACE} on '{SHELL_DESTINATION}' at '{SHELL_DEBUG_PATH}'");
    Ok(())
}
