//! Directory watcher for application directories using inotify and 200ms debouncing.
//!
//! Complies with K1.4: watches `$XDG_DATA_DIRS/applications` for creates, modifies,
//! atomic renames, deletes, and symlink changes with zero polling.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{debug, error, info, warn};

/// Internal message passed through the inotify channel to the debounce worker.
enum WatcherEvent {
    /// A relevant filesystem change occurred.
    FsEvent,
    /// Watcher is stopping.
    Stop,
}

/// Callback invoked when application directory changes settle.
pub type ChangeCallback = Arc<dyn Fn() + Send + Sync + 'static>;

/// Handle managing the background inotify filesystem watcher.
pub struct AppsWatcher {
    watcher: Option<RecommendedWatcher>,
    stop_tx: Option<mpsc::Sender<WatcherEvent>>,
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl AppsWatcher {
    /// Starts watching the specified directories for `.desktop` changes.
    ///
    /// Debounces bursts with a 200ms quiet window before invoking `on_changed`.
    ///
    /// # Errors
    /// Returns `notify::Error` if watcher initialization or directory watch fails.
    pub fn new<F>(dirs: &[PathBuf], on_changed: F) -> Result<Self, notify::Error>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel::<WatcherEvent>();
        let running = Arc::new(AtomicBool::new(true));

        let tx_clone = tx.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    if is_relevant_event(&event) {
                        let _ = tx_clone.send(WatcherEvent::FsEvent);
                    }
                }
                Err(err) => {
                    warn!("AppsWatcher filesystem watch error: {err}");
                }
            },
            Config::default(),
        )?;

        // Watch each existing applications directory
        for dir in dirs {
            if dir.is_dir() {
                debug!("AppsWatcher: watching directory '{}'", dir.display());
                if let Err(err) = watcher.watch(dir, RecursiveMode::Recursive) {
                    warn!("AppsWatcher: failed to watch '{}': {err}", dir.display());
                }
            }
        }

        let running_clone = Arc::clone(&running);
        let on_changed_arc = Arc::new(on_changed);

        let thread_handle = thread::Builder::new()
            .name("kynex-apps-watch".to_string())
            .spawn(move || {
                run_debounce_loop(rx, running_clone, on_changed_arc);
            })
            .map_err(|e| {
                error!("Failed to spawn watcher thread: {e}");
                notify::Error::generic(&e.to_string())
            })?;

        Ok(Self {
            watcher: Some(watcher),
            stop_tx: Some(tx),
            running,
            thread_handle: Some(thread_handle),
        })
    }
}

impl Drop for AppsWatcher {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(WatcherEvent::Stop);
        }
        drop(self.watcher.take());
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Checks whether a filesystem event affects `.desktop` files or directory structures.
fn is_relevant_event(event: &Event) -> bool {
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
            for path in &event.paths {
                if path.is_dir() {
                    return true;
                }
                if let Some(ext) = path.extension() {
                    if ext == "desktop" {
                        return true;
                    }
                }
                // Handle atomic rename temporary files (e.g. .desktop.XXXXXX or gvfs temp)
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.contains(".desktop") {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Runs the 200ms debounce loop on incoming change notifications.
#[allow(clippy::needless_pass_by_value)]
fn run_debounce_loop(
    rx: mpsc::Receiver<WatcherEvent>,
    running: Arc<AtomicBool>,
    on_changed: Arc<dyn Fn() + Send + Sync>,
) {
    const DEBOUNCE_TIME: Duration = Duration::from_millis(200);

    while running.load(Ordering::Relaxed) {
        // Block until first event in burst with ZERO polling (futex sleep)
        match rx.recv() {
            Ok(WatcherEvent::FsEvent) => {
                // Drain any additional rapid events until DEBOUNCE_TIME quiet period passes
                loop {
                    match rx.recv_timeout(DEBOUNCE_TIME) {
                        Ok(WatcherEvent::FsEvent) => {
                            // Another event arrived during debounce window; restart quiet timer
                        }
                        Ok(WatcherEvent::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            return;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // 200ms quiet window elapsed: fire single callback
                            info!("AppsWatcher: 200ms debounce elapsed, firing Changed callback");
                            on_changed();
                            break;
                        }
                    }
                }
            }
            Ok(WatcherEvent::Stop) | Err(mpsc::RecvError) => {
                break;
            }
        }
    }
}
