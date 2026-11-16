//! Audio management service implementing `org.kynexos.Daemon.Audio1`.
//!
//! Connects to `PulseAudio` / PipeWire-pulse via an asynchronous pure-Rust `AudioBackend` trait,
//! enforcing a 2-second subprocess timeout, clean environment allowlist, argv-only execution,
//! 4 KiB stream line buffering, 20/s rate limiting, caller UID verification, and exponential backoff
//! with a 5 restarts/minute cap.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kynex_audio::{
    parse_server_info_json, parse_sinks_json, parse_subscribe_line, resolve_audio_status,
    AudioError, AudioEvent, AudioStatus, BoundedLineReader, CoalesceAction, EventCoalescer,
};
use kynex_ipc::audio::AUDIO_MUTATION_RATE_LIMIT_PER_SEC;
use kynex_ipc::BoxFuture;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::Connection;

/// Compile-time fixed allowlist of absolute filesystem paths for the `pactl` executable.
pub const PACTL_ALLOWLIST: &[&str] = &["/usr/bin/pactl", "/bin/pactl"];

/// Compile-time fixed allowlist of absolute filesystem paths for the `setpriv` executable.
pub const SETPRIV_ALLOWLIST: &[&str] = &["/usr/bin/setpriv", "/bin/setpriv"];

/// Maximum restarts permitted within a 60-second window.
pub const MAX_RESTARTS_PER_MINUTE: usize = 5;

/// Subprocess execution timeout (2 seconds).
pub const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(2);

/// Environment variables permitted during backend subprocess execution.
pub const CLEAN_ENV_ALLOWLIST: &[&str] = &[
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "PULSE_SERVER",
    "HOME",
    "LANG",
    "LC_ALL",
];

#[cfg(any(test, feature = "test-hooks"))]
static PACTL_PATH_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Overrides the pactl path in test builds.
#[cfg(any(test, feature = "test-hooks"))]
pub fn set_pactl_path_override(path: Option<PathBuf>) {
    let mut lock = PACTL_PATH_OVERRIDE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *lock = path;
}

/// Resolves the absolute path to `pactl` from the fixed compile-time allowlist or test override.
///
/// In production builds, this NEVER consults `$PATH` or untrusted environment variables.
#[must_use]
pub fn resolve_pactl_path() -> Option<PathBuf> {
    #[cfg(any(test, feature = "test-hooks"))]
    {
        let lock = PACTL_PATH_OVERRIDE
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ref p) = *lock {
            return Some(p.clone());
        }
    }

    for &candidate in PACTL_ALLOWLIST {
        let p = Path::new(candidate);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// Resolves the absolute path to `setpriv` from the fixed compile-time allowlist.
#[must_use]
pub fn resolve_setpriv_path() -> Option<PathBuf> {
    for &candidate in SETPRIV_ALLOWLIST {
        let p = Path::new(candidate);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// Trait abstracting audio hardware and session mixer interaction.
pub trait AudioBackend: Send + Sync + std::fmt::Debug {
    /// Returns whether the backend is configured and available.
    fn is_available(&self) -> bool;

    /// Fetches the authoritative status of the active default sink.
    fn get_status(&self) -> BoxFuture<'_, Result<AudioStatus, AudioError>>;

    /// Sets master output volume (0-100).
    fn set_volume(&self, volume: u32) -> BoxFuture<'_, Result<(), AudioError>>;

    /// Sets master mute state.
    fn set_muted(&self, muted: bool) -> BoxFuture<'_, Result<(), AudioError>>;

    /// Spawns a long-running event subscribe process.
    fn spawn_subscribe(&self) -> BoxFuture<'_, Result<Child, AudioError>>;
}

/// Production audio backend delegating to `pactl`.
#[derive(Debug, Clone)]
pub struct PactlAudioBackend {
    pactl_path: Option<PathBuf>,
    setpriv_path: Option<PathBuf>,
    active_sink: Arc<RwLock<Option<String>>>,
}

impl PactlAudioBackend {
    /// Creates a new `pactl` backend instance, probing allowlisted paths.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pactl_path: resolve_pactl_path(),
            setpriv_path: resolve_setpriv_path(),
            active_sink: Arc::new(RwLock::new(None)),
        }
    }

    /// Creates a new backend with custom executable paths (e.g. for testing or isolated environments).
    #[must_use]
    pub fn new_with_paths(pactl_path: Option<PathBuf>, setpriv_path: Option<PathBuf>) -> Self {
        Self {
            pactl_path,
            setpriv_path,
            active_sink: Arc::new(RwLock::new(None)),
        }
    }

    /// Creates a `Command` with a sanitized environment and direct argv parameters.
    fn make_command(&self, args: &[&str]) -> Result<Command, AudioError> {
        let path = self.pactl_path.as_ref().ok_or(AudioError::MissingBackend)?;

        let mut cmd = Command::new(path);
        cmd.args(args);
        cmd.env_clear();

        for &key in CLEAN_ENV_ALLOWLIST {
            if let Ok(val) = std::env::var(key) {
                cmd.env(key, val);
            }
        }

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        Ok(cmd)
    }

    /// Runs a short subprocess call with a bounded 2-second timeout.
    async fn run_short_call(&self, args: &[&str]) -> Result<String, AudioError> {
        let mut cmd = self.make_command(args)?;

        let mut child = cmd.spawn().map_err(|e| {
            AudioError::ProcessError(format!("Failed to spawn pactl {args:?}: {e}"))
        })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        match tokio::time::timeout(SUBPROCESS_TIMEOUT, child.wait()).await {
            Ok(Ok(status)) => {
                let mut out_bytes = Vec::new();
                if let Some(mut out) = stdout {
                    let _ = out.read_to_end(&mut out_bytes).await;
                }
                let mut err_bytes = Vec::new();
                if let Some(mut err) = stderr {
                    let _ = err.read_to_end(&mut err_bytes).await;
                }

                if !status.success() {
                    let err_str = String::from_utf8_lossy(&err_bytes).to_string();
                    return Err(AudioError::ProcessError(format!(
                        "pactl exited with {status}: {err_str}"
                    )));
                }
                Ok(String::from_utf8_lossy(&out_bytes).to_string())
            }
            Ok(Err(e)) => Err(AudioError::ProcessError(e.to_string())),
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                Err(AudioError::Timeout)
            }
        }
    }
}

impl Default for PactlAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioBackend for PactlAudioBackend {
    fn is_available(&self) -> bool {
        self.pactl_path.as_ref().is_some_and(|p| p.is_file())
    }

    fn get_status(&self) -> BoxFuture<'_, Result<AudioStatus, AudioError>> {
        Box::pin(async move {
            if !self.is_available() {
                return Ok(AudioStatus::default());
            }

            let info_raw = self.run_short_call(&["--format=json", "info"]).await?;
            let server_info = parse_server_info_json(&info_raw)?;

            let sinks_raw = self
                .run_short_call(&["--format=json", "list", "sinks"])
                .await?;
            let sinks = parse_sinks_json(&sinks_raw)?;

            let target_sink = if !server_info.default_sink_name.is_empty() {
                server_info.default_sink_name.clone()
            } else if let Some(first) = sinks.first() {
                first.name.clone()
            } else {
                "@DEFAULT_SINK@".to_string()
            };
            *self.active_sink.write().await = Some(target_sink);

            Ok(resolve_audio_status(&server_info, &sinks))
        })
    }

    fn set_volume(&self, volume: u32) -> BoxFuture<'_, Result<(), AudioError>> {
        Box::pin(async move {
            let clamped = volume.clamp(0, 100);
            let vol_str = format!("{clamped}%");
            let sink = self
                .active_sink
                .read()
                .await
                .clone()
                .unwrap_or_else(|| "@DEFAULT_SINK@".to_string());
            self.run_short_call(&["set-sink-volume", &sink, &vol_str])
                .await?;
            Ok(())
        })
    }

    fn set_muted(&self, muted: bool) -> BoxFuture<'_, Result<(), AudioError>> {
        Box::pin(async move {
            let mute_str = if muted { "1" } else { "0" };
            let sink = self
                .active_sink
                .read()
                .await
                .clone()
                .unwrap_or_else(|| "@DEFAULT_SINK@".to_string());
            self.run_short_call(&["set-sink-mute", &sink, mute_str])
                .await?;
            Ok(())
        })
    }

    fn spawn_subscribe(&self) -> BoxFuture<'_, Result<Child, AudioError>> {
        Box::pin(async move {
            let pactl = self.pactl_path.as_ref().ok_or(AudioError::MissingBackend)?;
            let pactl_str = pactl.to_string_lossy().to_string();

            let mut cmd = if let Some(ref setpriv) = self.setpriv_path {
                let mut c = Command::new(setpriv);
                c.args(["--pdeathsig", "SIGKILL", "--", &pactl_str, "subscribe"]);
                c
            } else {
                let mut c = Command::new(pactl);
                c.arg("subscribe");
                c
            };

            cmd.env_clear();
            for &key in CLEAN_ENV_ALLOWLIST {
                if let Ok(val) = std::env::var(key) {
                    cmd.env(key, val);
                }
            }

            cmd.stdin(Stdio::null());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            cmd.kill_on_drop(true);

            cmd.spawn().map_err(|e| {
                AudioError::ProcessError(format!("Failed to spawn pactl subscribe: {e}"))
            })
        })
    }
}

/// Rate limiting bucket for global audio mutations (20 calls/second).
#[derive(Debug)]
struct AudioRateBucket {
    window_start: Instant,
    count: u32,
}

impl AudioRateBucket {
    fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            count: 0,
        }
    }

    fn check_and_inc(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.window_start);
        if elapsed >= Duration::from_secs(1) {
            self.window_start = now;
            self.count = 1;
            true
        } else if self.count < AUDIO_MUTATION_RATE_LIMIT_PER_SEC {
            self.count += 1;
            true
        } else {
            false
        }
    }
}

/// D-Bus object implementation for `org.kynexos.Daemon.Audio1`.
#[derive(Debug, Clone)]
pub struct AudioServer {
    state: Arc<RwLock<AudioStatus>>,
    backend: Arc<dyn AudioBackend>,
    rate_limiter: Arc<Mutex<AudioRateBucket>>,
    emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
}

impl AudioServer {
    /// Creates a new `AudioServer` backed by the specified audio backend.
    #[must_use]
    pub fn new(backend: Arc<dyn AudioBackend>) -> Self {
        Self {
            state: Arc::new(RwLock::new(AudioStatus::default())),
            backend,
            rate_limiter: Arc::new(Mutex::new(AudioRateBucket::new(Instant::now()))),
            emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Creates a default `AudioServer` with the standard `PactlAudioBackend`.
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(Arc::new(PactlAudioBackend::new()))
    }

    /// Sets the D-Bus signal emitter for broadcasting property changes.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        let mut lock = self.emitter.lock().await;
        *lock = Some(emitter);
    }

    /// Updates internal state and broadcasts `PropertiesChanged` for modified properties.
    pub async fn update_state(&self, new_state: AudioStatus) {
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

        let lock = self.emitter.lock().await;
        if let Some(ref em) = *lock {
            if old.present != new_state.present {
                let _ = self.present_changed(em).await;
            }
            if old.volume != new_state.volume {
                let _ = self.volume_changed(em).await;
            }
            if old.muted != new_state.muted {
                let _ = self.muted_changed(em).await;
            }
            if old.device_name != new_state.device_name {
                let _ = self.device_name_changed(em).await;
            }
        }
    }

    /// Returns a snapshot of the current audio status.
    pub async fn get_snapshot(&self) -> AudioStatus {
        self.state.read().await.clone()
    }

    /// Verifies caller UID matches daemon session UID.
    async fn verify_caller(
        &self,
        hdr: &Header<'_>,
        conn: &Connection,
    ) -> Result<(), zbus::fdo::Error> {
        let sender = hdr
            .sender()
            .ok_or_else(|| zbus::fdo::Error::AccessDenied("Missing message sender".into()))?;

        let dbus_proxy = DBusProxy::new(conn).await?;
        let caller_uid = dbus_proxy
            .get_connection_unix_user(sender.clone().into())
            .await?;

        let daemon_uid = rustix::process::getuid().as_raw();
        if caller_uid != daemon_uid {
            return Err(zbus::fdo::Error::AccessDenied(format!(
                "Caller UID {caller_uid} does not match daemon UID {daemon_uid}"
            )));
        }
        Ok(())
    }

    /// Checks and increments the global mutation rate limiter.
    async fn check_rate_limit(&self) -> Result<(), zbus::fdo::Error> {
        let mut limiter = self.rate_limiter.lock().await;
        if !limiter.check_and_inc(Instant::now()) {
            return Err(zbus::fdo::Error::LimitsExceeded(
                "Audio mutation rate limit exceeded (max 20 calls/sec)".into(),
            ));
        }
        Ok(())
    }
}

#[zbus::interface(name = "org.kynexos.Daemon.Audio1")]
impl AudioServer {
    /// Whether an operational audio backend is detected and active.
    #[zbus(property)]
    pub async fn present(&self) -> bool {
        self.state.read().await.present
    }

    /// Master output volume level clamped between 0 and 100.
    #[zbus(property)]
    pub async fn volume(&self) -> u32 {
        self.state.read().await.volume
    }

    /// Master mute state.
    #[zbus(property)]
    pub async fn muted(&self) -> bool {
        self.state.read().await.muted
    }

    /// Sanitized name of the active default audio output device.
    #[zbus(property)]
    pub async fn device_name(&self) -> String {
        self.state.read().await.device_name.clone()
    }

    /// Sets the master output volume level (0-100).
    ///
    /// # Errors
    /// Returns `InvalidArgs` if volume > 100, `LimitsExceeded` on rate limiting,
    /// or `Failed` on subprocess error.
    pub async fn set_volume(
        &self,
        volume: u32,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), zbus::fdo::Error> {
        self.verify_caller(&hdr, conn).await?;
        self.check_rate_limit().await?;

        if volume > 100 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Volume level must be between 0 and 100".into(),
            ));
        }

        self.backend
            .set_volume(volume)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to set audio volume: {e}")))?;

        // Read back immediately to verify state and synchronize
        if let Ok(updated) = self.backend.get_status().await {
            self.update_state(updated).await;
        }

        Ok(())
    }

    /// Sets the master output mute state.
    ///
    /// # Errors
    /// Returns `LimitsExceeded` on rate limiting or `Failed` on subprocess error.
    pub async fn set_muted(
        &self,
        muted: bool,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), zbus::fdo::Error> {
        self.verify_caller(&hdr, conn).await?;
        self.check_rate_limit().await?;

        self.backend
            .set_muted(muted)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to set audio mute: {e}")))?;

        // Read back immediately to verify state and synchronize
        if let Ok(updated) = self.backend.get_status().await {
            self.update_state(updated).await;
        }

        Ok(())
    }
}

/// Runs the background audio management service.
///
/// Manages the long-running `pactl subscribe` child process, event coalescing,
/// exponential backoff restarts with a 5/minute cap, and status synchronization.
#[allow(clippy::too_many_lines)]
pub async fn run_audio_service(server: AudioServer, backend: Arc<dyn AudioBackend>) {
    info!("Starting audio background service");

    if !backend.is_available() {
        info!("No audio backend available; running Audio1 with Present=false");
        server.update_state(AudioStatus::default()).await;
        return;
    }

    // Initial status query
    match backend.get_status().await {
        Ok(status) => {
            debug!("Initial audio status: {:?}", status);
            server.update_state(status).await;
        }
        Err(e) => {
            warn!("Failed to query initial audio status: {e}");
            server.update_state(AudioStatus::default()).await;
        }
    }

    let mut restart_times: Vec<Instant> = Vec::new();
    let mut consecutive_failures: u32 = 0;
    let mut coalescer = EventCoalescer::default();
    let start_instant = Instant::now();

    loop {
        let mut child = match backend.spawn_subscribe().await {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to spawn audio subscribe child: {e}");
                consecutive_failures = consecutive_failures.saturating_add(1);
                restart_times.retain(|t| t.elapsed() < Duration::from_secs(60));
                if restart_times.len() >= MAX_RESTARTS_PER_MINUTE {
                    error!(
                        "Audio subscribe child failed {} times in 60s; disabling audio backend",
                        restart_times.len()
                    );
                    server.update_state(AudioStatus::default()).await;
                    break;
                }
                restart_times.push(Instant::now());

                let backoff_ms = 100u64.saturating_mul(1u64 << consecutive_failures.min(5));
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                continue;
            }
        };

        // Reset failure count on successful spawn
        consecutive_failures = 0;

        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            let _ = child.wait().await;
            continue;
        };

        let mut reader = BoundedLineReader::new();
        let mut buf = [0u8; 8192];
        let mut timer: Option<Pin<Box<tokio::time::Sleep>>> = None;

        loop {
            tokio::select! {
                () = async {
                    match timer.as_mut() {
                        Some(sleep) => sleep.await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    timer = None;
                    if let Ok(updated) = backend.get_status().await {
                        server.update_state(updated).await;
                    }
                    let now_ms = u64::try_from(start_instant.elapsed().as_millis()).unwrap_or(0);
                    if let Some(next_delay) = coalescer.on_debounced_run(now_ms) {
                        timer = Some(Box::pin(tokio::time::sleep(Duration::from_millis(next_delay))));
                    }
                }
                read_res = stdout.read(&mut buf) => {
                    match read_res {
                        Ok(0) => {
                            debug!("Audio subscribe stream reached EOF");
                            if coalescer.is_scheduled() {
                                if let Ok(updated) = backend.get_status().await {
                                    server.update_state(updated).await;
                                }
                                let now_ms = u64::try_from(start_instant.elapsed().as_millis()).unwrap_or(0);
                                let _ = coalescer.on_debounced_run(now_ms);
                            }
                            break;
                        }
                        Ok(n) => {
                            let now_ms = u64::try_from(start_instant.elapsed().as_millis()).unwrap_or(0);
                            reader.feed_each(&buf[..n], |line_res| {
                                if let Ok(line) = line_res {
                                    if let Some(event) = parse_subscribe_line(line) {
                                        if matches!(event, AudioEvent::SinkChanged(_) | AudioEvent::ServerChanged) {
                                            match coalescer.handle_event(now_ms) {
                                                CoalesceAction::ScheduleDebounce(delay) => {
                                                    timer = Some(Box::pin(tokio::time::sleep(Duration::from_millis(delay))));
                                                }
                                                CoalesceAction::AlreadyScheduled => {}
                                            }
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            warn!("Error reading audio subscribe stream: {e}");
                            break;
                        }
                    }
                }
            }
        }

        // Child process exited or failed; reap it cleanly so no zombies remain
        let _ = child.kill().await;
        let _ = child.wait().await;

        restart_times.retain(|t| t.elapsed() < Duration::from_secs(60));
        if restart_times.len() >= MAX_RESTARTS_PER_MINUTE {
            error!(
                "Audio subscribe process exited {} times in 60s; disabling audio backend",
                restart_times.len()
            );
            server.update_state(AudioStatus::default()).await;
            break;
        }

        restart_times.push(Instant::now());
        consecutive_failures = consecutive_failures.saturating_add(1);
        let backoff_ms = 100u64.saturating_mul(1u64 << consecutive_failures.min(5));
        debug!("Restarting audio subscribe stream after {backoff_ms} ms");
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }
}
