//! Pinned applications service implementing `org.kynexos.Daemon.Pins1`.
//!
//! Provides atomic configuration persistence to `$XDG_CONFIG_HOME/kynex/pinned.toml`,
//! caller UID authorization, global 10/s mutation rate limiting, and safe error recovery.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection};

/// Maximum allowable pinned applications.
pub const MAX_PINS: usize = 128;

/// Maximum allowable file size for `pinned.toml` (64 KiB).
pub const MAX_PINNED_FILE_SIZE: u64 = 64 * 1024;

/// Maximum allowable bytes for serialized `pinned.toml` (64 KiB).
pub const MAX_PINNED_BYTES: usize = 64 * 1024;

/// Global mutation rate limit (mutations per second).
pub const PINS_MUTATION_RATE_LIMIT_PER_SEC: u32 = 10;

/// Persistent file representation for `pinned.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PinnedFileSchema {
    /// Schema version (fixed to 1).
    pub schema_version: u32,
    /// Ordered list of pinned application IDs.
    pub pins: Vec<String>,
}

impl Default for PinnedFileSchema {
    fn default() -> Self {
        Self {
            schema_version: 1,
            pins: Vec::new(),
        }
    }
}

/// Validate application ID against allowed character set and length bounds.
#[must_use]
pub fn validate_app_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 128 {
        return false;
    }
    id.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Rate limiting bucket for global mutations.
#[derive(Debug)]
struct MutationRateBucket {
    window_start: Instant,
    count: u32,
}

impl MutationRateBucket {
    fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            count: 0,
        }
    }

    fn check_and_inc(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.window_start);
        if elapsed >= std::time::Duration::from_secs(1) {
            self.window_start = now;
            self.count = 1;
            true
        } else if self.count < PINS_MUTATION_RATE_LIMIT_PER_SEC {
            self.count += 1;
            true
        } else {
            false
        }
    }
}

/// Shared state for pinned applications.
#[derive(Debug)]
pub struct PinsSharedState {
    /// Ordered list of pinned application IDs.
    pins: RwLock<Vec<String>>,
    /// Path to `pinned.toml`.
    config_path: PathBuf,
    /// Rate limiter for mutations.
    rate_limiter: Mutex<MutationRateBucket>,
    /// Unique counter for temporary files.
    temp_counter: AtomicU64,
}

impl PinsSharedState {
    /// Create new shared state loading from the specified config path.
    #[must_use]
    pub fn new(config_path: PathBuf) -> Self {
        let loaded = Self::load_from_path(&config_path);
        Self {
            pins: RwLock::new(loaded),
            config_path,
            rate_limiter: Mutex::new(MutationRateBucket::new(Instant::now())),
            temp_counter: AtomicU64::new(1),
        }
    }

    /// Default constructor pointing to `$XDG_CONFIG_HOME/kynex/pinned.toml`.
    #[must_use]
    pub fn new_default() -> Self {
        let config_dir = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"));
        let config_path = config_dir.join("kynex/pinned.toml");
        Self::new(config_path)
    }

    /// Get a clone of the current pinned application IDs.
    pub async fn get_pins(&self) -> Vec<String> {
        self.pins.read().await.clone()
    }

    /// Load pinned applications from disk, safely handling errors by renaming corrupt files to `.bad`.
    pub fn load_from_path(path: &Path) -> Vec<String> {
        if !path.exists() {
            return Vec::new();
        }

        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                warn!("Pins: failed to stat {path:?}: {e}");
                return Vec::new();
            }
        };

        // If file is oversized (> 64 KiB) or not a regular file, rename to .bad
        if !meta.is_file() || meta.len() > MAX_PINNED_FILE_SIZE {
            warn!("Pins: {path:?} is oversized or not regular file; renaming to .bad");
            Self::quarantine_bad_file(path);
            return Vec::new();
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Pins: failed to read {path:?}: {e}; renaming to .bad");
                Self::quarantine_bad_file(path);
                return Vec::new();
            }
        };

        let parsed: PinnedFileSchema = match toml::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                warn!("Pins: failed to parse TOML {path:?}: {e}; renaming to .bad");
                Self::quarantine_bad_file(path);
                return Vec::new();
            }
        };

        // Filter and validate IDs
        let mut valid_pins = Vec::new();
        for id in parsed.pins {
            let trimmed = id.trim();
            if validate_app_id(trimmed) && !valid_pins.contains(&trimmed.to_string()) {
                valid_pins.push(trimmed.to_string());
                if valid_pins.len() >= MAX_PINS {
                    break;
                }
            }
        }

        valid_pins
    }

    fn quarantine_bad_file(path: &Path) {
        let bad_path = path.with_extension("toml.bad");
        let _ = fs::rename(path, &bad_path);
    }

    /// Atomically persist current pins to disk (temp file in same directory, mode 0600, fsync, rename).
    ///
    /// # Errors
    /// Returns `std::io::Error` if serialization exceeds 64 KiB or filesystem operations fail.
    pub async fn persist(&self) -> Result<(), std::io::Error> {
        let pins = self.pins.read().await.clone();
        let schema = PinnedFileSchema {
            schema_version: 1,
            pins,
        };

        let toml_data = toml::to_string_pretty(&schema)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if toml_data.len() > MAX_PINNED_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Serialized pinned.toml exceeds 64 KiB",
            ));
        }

        let parent = self.config_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let count = self.temp_counter.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let temp_filename = format!(".pinned.toml.tmp.{pid}.{count}");
        let temp_path = parent.join(temp_filename);

        // Open with mode 0600
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp_path)?;

        file.write_all(toml_data.as_bytes())?;
        file.sync_all()?;
        drop(file);

        if let Err(e) = fs::rename(&temp_path, &self.config_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(e);
        }

        Ok(())
    }
}

/// D-Bus object implementation for `org.kynexos.Daemon.Pins1`.
#[derive(Debug, Clone)]
pub struct PinsServer {
    shared: Arc<PinsSharedState>,
    emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
}

impl PinsServer {
    /// Create new server instance from shared state.
    #[must_use]
    pub fn new(shared: Arc<PinsSharedState>) -> Self {
        Self {
            shared,
            emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Default constructor pointing to `$XDG_CONFIG_HOME/kynex/pinned.toml`.
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(Arc::new(PinsSharedState::new_default()))
    }

    /// Set signal emitter for broadcasting `Changed` signals.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        let mut lock = self.emitter.lock().await;
        *lock = Some(emitter);
    }

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

    async fn check_rate_limit(&self) -> Result<(), zbus::fdo::Error> {
        let mut bucket = self.shared.rate_limiter.lock().await;
        if !bucket.check_and_inc(Instant::now()) {
            return Err(zbus::fdo::Error::LimitsExceeded(
                "Pins mutation rate limit exceeded (max 10/s)".into(),
            ));
        }
        Ok(())
    }

    async fn emit_changed(&self) {
        let lock = self.emitter.lock().await;
        if let Some(ref emitter) = *lock {
            let _ = Self::changed(emitter).await;
        }
    }
}

#[interface(name = "org.kynexos.Daemon.Pins1")]
impl PinsServer {
    /// Signal emitted when pinned applications change.
    #[zbus(signal)]
    pub async fn changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Returns the ordered list of pinned application IDs.
    async fn get(&self) -> zbus::fdo::Result<Vec<String>> {
        let pins = self.shared.pins.read().await;
        Ok(pins.clone())
    }

    /// Pin an application ID.
    async fn pin(
        &self,
        id: String,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<()> {
        self.verify_caller(&hdr, conn).await?;
        self.check_rate_limit().await?;

        let trimmed = id.trim();
        if !validate_app_id(trimmed) {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Invalid application ID format or length".into(),
            ));
        }

        {
            let mut pins = self.shared.pins.write().await;
            if pins.iter().any(|p| p == trimmed) {
                return Ok(());
            }

            if pins.len() >= MAX_PINS {
                return Err(zbus::fdo::Error::LimitsExceeded(
                    "Maximum number of pinned applications (128) reached".into(),
                ));
            }

            pins.push(trimmed.to_string());
        }

        self.shared
            .persist()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to persist pinned.toml: {e}")))?;

        info!("Pins1: pinned application '{trimmed}'");
        self.emit_changed().await;
        Ok(())
    }

    /// Unpin an application ID.
    async fn unpin(
        &self,
        id: String,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<()> {
        self.verify_caller(&hdr, conn).await?;
        self.check_rate_limit().await?;

        let trimmed = id.trim();
        let changed = {
            let mut pins = self.shared.pins.write().await;
            let before = pins.len();
            pins.retain(|p| p != trimmed);
            pins.len() != before
        };

        if changed {
            self.shared.persist().await.map_err(|e| {
                zbus::fdo::Error::Failed(format!("Failed to persist pinned.toml: {e}"))
            })?;

            info!("Pins1: unpinned application '{trimmed}'");
            self.emit_changed().await;
        }

        Ok(())
    }

    /// Move a pinned application ID to `index`.
    #[zbus(name = "Move")]
    async fn move_pin(
        &self,
        id: String,
        index: u32,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<()> {
        self.verify_caller(&hdr, conn).await?;
        self.check_rate_limit().await?;

        let trimmed = id.trim();
        let changed = {
            let mut pins = self.shared.pins.write().await;
            if let Some(pos) = pins.iter().position(|p| p == trimmed) {
                let item = pins.remove(pos);
                let target = (index as usize).min(pins.len());
                pins.insert(target, item);
                pos != target
            } else {
                false
            }
        };

        if changed {
            self.shared.persist().await.map_err(|e| {
                zbus::fdo::Error::Failed(format!("Failed to persist pinned.toml: {e}"))
            })?;

            info!("Pins1: moved application '{trimmed}' to index {index}");
            self.emit_changed().await;
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::assert_is_empty)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_validate_app_id() {
        assert!(validate_app_id("org.gnome.Terminal"));
        assert!(validate_app_id("app-1_test.desktop"));
        assert!(!validate_app_id(""));
        assert!(!validate_app_id("   "));
        assert!(!validate_app_id("foo/bar"));
        assert!(!validate_app_id("foo\\bar"));
        assert!(!validate_app_id("foo\0bar"));
        assert!(!validate_app_id("foo\nbar"));
        assert!(!validate_app_id(&"a".repeat(129)));
    }

    #[test]
    fn test_rate_limit_bucket() {
        let now = Instant::now();
        let mut bucket = MutationRateBucket::new(now);
        for _ in 0..10 {
            assert!(bucket.check_and_inc(now));
        }
        assert!(!bucket.check_and_inc(now));

        let future = now + std::time::Duration::from_millis(1100);
        assert!(bucket.check_and_inc(future));
    }

    #[tokio::test]
    async fn test_pins_shared_state_persistence_and_limits() {
        let tmp = tempdir().unwrap();
        let config_path = tmp.path().join("pinned.toml");

        let state = PinsSharedState::new(config_path.clone());
        assert!(state.get_pins().await.is_empty());

        // Oversized payload check
        {
            let mut pins = state.pins.write().await;
            for i in 0..1000 {
                pins.push(format!("app_{i}_{}", "x".repeat(100)));
            }
        }
        assert!(state.persist().await.is_err());

        // Valid persist
        {
            let mut pins = state.pins.write().await;
            pins.clear();
            pins.push("org.gnome.Terminal".to_string());
            pins.push("org.gnome.Calculator".to_string());
        }
        assert!(state.persist().await.is_ok());

        // Reload from path
        let reloaded = PinsSharedState::load_from_path(&config_path);
        assert_eq!(reloaded, vec!["org.gnome.Terminal", "org.gnome.Calculator"]);

        // Oversized file on disk returns empty
        fs::write(&config_path, vec![b'a'; MAX_PINNED_BYTES + 1]).unwrap();
        let oversized = PinsSharedState::load_from_path(&config_path);
        assert!(oversized.is_empty());

        // Invalid TOML gets quarantined to .bad
        fs::write(&config_path, "not a toml file [[[").unwrap();
        let quarantined = PinsSharedState::load_from_path(&config_path);
        assert!(quarantined.is_empty());
        assert!(!config_path.exists());
        assert!(tmp.path().join("pinned.toml.bad").exists());
    }

    #[test]
    fn test_pins_constructors() {
        let server = PinsServer::new_default();
        let _ = format!("{server:?}");
        let state = PinsSharedState::new_default();
        let _ = format!("{state:?}");
    }
}
