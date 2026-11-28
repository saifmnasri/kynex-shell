//! Audio client model exposing master volume and sink status to QML via `cxx-qt`.
#![allow(clippy::too_many_arguments)]

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use futures_util::StreamExt;
use kynex_ipc::audio::{Audio1Proxy, AUDIO_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use kynex_shell_core::coordinates::{OutputGeometry, PanelSize, Point, SurfaceAnchors};
use kynex_shell_core::{AudioState, DebounceAction, SliderDebouncer};
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tracing::{debug, info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::names::WellKnownName;
use zbus::{MatchRule, MessageStream};

/// Command dispatched from QML to the asynchronous audio task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCommand {
    /// Sets master volume clamped to 0..=100.
    SetVolume(u32),
    /// Sets mute status.
    SetMuted(bool),
}

/// Returns current monotonic milliseconds.
#[must_use]
pub fn current_monotonic_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(0)
}

/// CXX-Qt bridge module for the `AudioModel` `QObject`.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Alias to Qt QString
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Audio status model exposed to QML.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, present, READ, NOTIFY)]
        #[qproperty(i32, volume, READ, NOTIFY)]
        #[qproperty(bool, muted, READ, NOTIFY)]
        #[qproperty(QString, device_name, READ, NOTIFY)]
        #[qproperty(bool, open)]
        type AudioModel = super::AudioModelRust;

        /// Starts the background D-Bus listener connecting to `org.kynexos.Daemon`.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Toggles open/closed state of the Quick Settings popup.
        #[qinvokable]
        #[cxx_name = "toggleQuickSettings"]
        fn toggle_quick_settings(self: Pin<&mut Self>);

        /// Sets master volume (0..=100) with rate limiting (<= 20/s) and guaranteed final value.
        #[qinvokable]
        #[cxx_name = "setVolume"]
        fn set_volume(self: Pin<&mut Self>, volume: i32);

        /// Sets mute state.
        #[qinvokable]
        #[cxx_name = "setMuted"]
        fn set_muted(self: Pin<&mut Self>, muted: bool);

        /// Computes popup placement X position using pure anchor placement.
        #[qinvokable]
        #[cxx_name = "computePlacementX"]
        fn compute_placement_x(
            self: &AudioModel,
            anchors_bits: u32,
            panel_width: f64,
            panel_height: f64,
            anchor_x: f64,
            anchor_y: f64,
            anchor_width: f64,
            anchor_height: f64,
            popup_width: f64,
            popup_height: f64,
            screen_width: f64,
            screen_height: f64,
        ) -> f64;

        /// Computes popup placement Y position using pure anchor placement.
        #[qinvokable]
        #[cxx_name = "computePlacementY"]
        fn compute_placement_y(
            self: &AudioModel,
            anchors_bits: u32,
            panel_width: f64,
            panel_height: f64,
            anchor_x: f64,
            anchor_y: f64,
            anchor_width: f64,
            anchor_height: f64,
            popup_width: f64,
            popup_height: f64,
            screen_width: f64,
            screen_height: f64,
        ) -> f64;

        /// Records first frame rendered after quick settings open request.
        #[qinvokable]
        #[cxx_name = "recordQuickSettingsOpened"]
        fn record_quick_settings_opened(self: &AudioModel);
    }

    impl cxx_qt::Threading for AudioModel {}
}

/// Backing data for the `AudioModel` `QObject`.
#[derive(Default)]
pub struct AudioModelRust {
    present: bool,
    volume: i32,
    muted: bool,
    device_name: QString,
    open: bool,
    cmd_tx: Option<UnboundedSender<AudioCommand>>,
}

impl qobject::AudioModel {
    /// Initialize the background D-Bus client connecting to `kynex-daemon`.
    pub fn init(mut self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        let (cmd_tx, cmd_rx) = unbounded_channel();
        self.as_mut().rust_mut().cmd_tx = Some(cmd_tx);

        tokio::spawn(async move {
            run_audio_listener(qt_thread, cmd_rx).await;
        });
    }

    /// Toggles the quick settings open state.
    pub fn toggle_quick_settings(mut self: Pin<&mut Self>) {
        let cur = self.rust().open;
        self.as_mut().set_open(!cur);
    }

    /// Sets master volume (0..=100), debounced before sending over D-Bus.
    pub fn set_volume(self: Pin<&mut Self>, volume: i32) {
        let clamped = u32::try_from(volume.clamp(0, 100)).unwrap_or(0);
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(AudioCommand::SetVolume(clamped));
        }
    }

    /// Sets mute state.
    pub fn set_muted(self: Pin<&mut Self>, muted: bool) {
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(AudioCommand::SetMuted(muted));
        }
    }

    /// Computes popup placement X position using pure anchor placement.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_placement_x(
        &self,
        anchors_bits: u32,
        panel_width: f64,
        panel_height: f64,
        anchor_x: f64,
        anchor_y: f64,
        anchor_width: f64,
        anchor_height: f64,
        popup_width: f64,
        popup_height: f64,
        screen_width: f64,
        screen_height: f64,
    ) -> f64 {
        let anchors = SurfaceAnchors::from_bits(anchors_bits);
        let panel = PanelSize {
            width: panel_width,
            height: panel_height,
        };
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: screen_width,
            height: screen_height,
            scale: 1.0,
        };
        let anchor = Point::new(anchor_x, anchor_y);
        let pt = kynex_shell_core::compute_quick_settings_placement(
            anchors,
            panel,
            anchor,
            anchor_width,
            anchor_height,
            popup_width,
            popup_height,
            screen,
        );
        pt.x
    }

    /// Computes popup placement Y position using pure anchor placement.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_placement_y(
        &self,
        anchors_bits: u32,
        panel_width: f64,
        panel_height: f64,
        anchor_x: f64,
        anchor_y: f64,
        anchor_width: f64,
        anchor_height: f64,
        popup_width: f64,
        popup_height: f64,
        screen_width: f64,
        screen_height: f64,
    ) -> f64 {
        let anchors = SurfaceAnchors::from_bits(anchors_bits);
        let panel = PanelSize {
            width: panel_width,
            height: panel_height,
        };
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: screen_width,
            height: screen_height,
            scale: 1.0,
        };
        let anchor = Point::new(anchor_x, anchor_y);
        let pt = kynex_shell_core::compute_quick_settings_placement(
            anchors,
            panel,
            anchor,
            anchor_width,
            anchor_height,
            popup_width,
            popup_height,
            screen,
        );
        pt.y
    }
    /// Records first frame rendered after quick settings open request.
    pub fn record_quick_settings_opened(&self) {
        let frame_ns = crate::shim::ffi::kynex_steady_clock_ns();
        tracing::info!(
            target: "kynex_perf",
            event = "quick_settings_frame_swapped",
            frame_ns = frame_ns,
            "QuickSettings frame swapped: frame_ns={frame_ns}"
        );
    }
}

/// Dispatches an `AudioState` snapshot to the Qt GUI main thread.
pub fn dispatch_state(qt_thread: &cxx_qt::CxxQtThread<qobject::AudioModel>, state: &AudioState) {
    let volume = i32::try_from(state.volume).unwrap_or(0);
    let dev_name = QString::from(state.device_name.as_str());
    let present = state.present;
    let muted = state.muted;
    let _ = qt_thread.queue(move |mut qobj| {
        let (chg_pres, chg_vol, chg_mut, chg_dev) = {
            let mut r = qobj.as_mut().rust_mut();
            let chg_pres = r.present != present;
            let chg_vol = r.volume != volume;
            let chg_mut = r.muted != muted;
            let chg_dev = r.device_name != dev_name;

            r.present = present;
            r.volume = volume;
            r.muted = muted;
            r.device_name = dev_name;
            (chg_pres, chg_vol, chg_mut, chg_dev)
        };

        if chg_pres {
            qobj.as_mut().present_changed();
        }
        if chg_vol {
            let frame_ns = crate::shim::ffi::kynex_steady_clock_ns();
            tracing::info!(
                target: "kynex_perf",
                event = "audio_volume_updated",
                frame_ns = frame_ns,
                volume = volume,
                "Audio volume updated: frame_ns={frame_ns} volume={volume}"
            );
            qobj.as_mut().volume_changed();
        }
        if chg_mut {
            qobj.as_mut().muted_changed();
        }
        if chg_dev {
            qobj.as_mut().device_name_changed();
        }
    });
}

/// Reads the current snapshot from `Audio1Proxy`, converting to `AudioState`.
pub async fn fetch_audio_state(proxy: &Audio1Proxy<'_>) -> AudioState {
    let present = proxy.present().await.unwrap_or(false);
    let volume = proxy.volume().await.unwrap_or(0);
    let muted = proxy.muted().await.unwrap_or(false);
    let device_name = proxy.device_name().await.unwrap_or_default();
    AudioState::from_raw(present, volume, muted, &device_name)
}

/// Attempts to connect to `org.kynexos.Daemon` if present on the bus.
async fn create_daemon_proxy<'a>(
    conn: &'a zbus::Connection,
    dbus_proxy: &DBusProxy<'_>,
) -> Option<Audio1Proxy<'a>> {
    let wk_name: Result<WellKnownName<'_>, _> = DAEMON_DESTINATION.try_into();
    let has_daemon = match wk_name {
        Ok(ref name) => dbus_proxy
            .get_name_owner(zbus::names::BusName::WellKnown(name.as_ref()))
            .await
            .is_ok(),
        Err(_) => false,
    };

    if has_daemon {
        Audio1Proxy::builder(conn)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .ok()
    } else {
        None
    }
}

/// Initializes a match-rule signal stream on `AUDIO_PATH`.
async fn init_signal_stream(conn: &zbus::Connection) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path(AUDIO_PATH)
        .build()
        .ok()?;
    MessageStream::for_match_rule(rule, conn, None).await.ok()
}

/// D-Bus interface for controlling Quick Settings flyout surface.
#[derive(Clone)]
pub struct QuickSettingsController {
    qt_thread: cxx_qt::CxxQtThread<qobject::AudioModel>,
}

#[zbus::interface(name = "org.kynexos.Shell.QuickSettings1")]
impl QuickSettingsController {
    /// Toggles the open/closed state of the Quick Settings surface.
    fn toggle(&self) {
        let _ = self.qt_thread.queue(|mut qobj| {
            let cur = qobj.rust().open;
            qobj.as_mut().set_open(!cur);
        });
    }

    /// Opens the Quick Settings surface.
    fn open(&self) {
        let _ = self.qt_thread.queue(|mut qobj| {
            qobj.as_mut().set_open(true);
        });
    }

    /// Closes the Quick Settings surface.
    fn close(&self) {
        let _ = self.qt_thread.queue(|mut qobj| {
            qobj.as_mut().set_open(false);
        });
    }
}

/// Background asynchronous listener connecting to `org.kynexos.Daemon.Audio1`.
#[allow(clippy::too_many_lines)]
pub async fn run_audio_listener(
    qt_thread: cxx_qt::CxxQtThread<qobject::AudioModel>,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<AudioCommand>,
) {
    let session_conn = match crate::shared_session_conn().await {
        Ok(c) => c,
        Err(err) => {
            warn!("AudioModel failed to connect to session bus: {err}");
            dispatch_state(&qt_thread, &AudioState::default());
            return;
        }
    };

    let _ = session_conn
        .object_server()
        .at(
            "/org/kynexos/Shell/QuickSettings",
            QuickSettingsController {
                qt_thread: qt_thread.clone(),
            },
        )
        .await;
    let _ = session_conn.request_name("org.kynexos.Shell").await;

    #[cfg(feature = "bench")]
    if std::env::var("KYNEX_BENCH_OPEN_QUICK_SETTINGS").unwrap_or_default() == "1" {
        let _ = qt_thread.queue(|mut qobj| {
            qobj.as_mut().set_open(true);
        });
    }

    let dbus_proxy = match DBusProxy::new(&session_conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("AudioModel failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
        .ok();

    let mut signal_stream = init_signal_stream(&session_conn).await;
    let mut audio_proxy = create_daemon_proxy(&session_conn, &dbus_proxy).await;

    if let Some(ref proxy) = audio_proxy {
        let state = fetch_audio_state(proxy).await;
        info!(
            "AudioModel connected: present={}, volume={}, muted={}, device={}",
            state.present, state.volume, state.muted, state.device_name
        );
        dispatch_state(&qt_thread, &state);
        crate::debug::get_debug_hub()
            .update_model_readiness(|m| m.audio = true)
            .await;
    } else {
        info!("kynex-daemon not found on bus; setting present=false");
        dispatch_state(&qt_thread, &AudioState::default());
    }

    let mut debouncer = SliderDebouncer::default();
    let timer_sleep = tokio::time::sleep(Duration::MAX);
    tokio::pin!(timer_sleep);
    let mut timer_active = false;

    loop {
        tokio::select! {
            // Commands from QML
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    AudioCommand::SetVolume(vol) => {
                        let now_ms = current_monotonic_ms();
                        match debouncer.handle_input(vol, now_ms) {
                            DebounceAction::SendImmediate(v) => {
                                if let Some(ref proxy) = audio_proxy {
                                    if let Err(e) = proxy.set_volume(v).await {
                                        warn!("Failed to set volume on daemon: {e}");
                                    }
                                }
                            }
                            DebounceAction::ScheduleTimer { delay_ms, .. } => {
                                timer_sleep.as_mut().reset(tokio::time::Instant::now() + Duration::from_millis(delay_ms));
                                timer_active = true;
                            }
                            DebounceAction::UpdatePending(_) => {
                                // Handled internally by debouncer
                            }
                        }
                    }
                    AudioCommand::SetMuted(muted) => {
                        if let Some(ref proxy) = audio_proxy {
                            if let Err(e) = proxy.set_muted(muted).await {
                                warn!("Failed to set muted on daemon: {e}");
                            }
                        }
                    }
                }
            }

            // Debounce timer elapsed
            () = &mut timer_sleep, if timer_active => {
                timer_active = false;
                let now_ms = current_monotonic_ms();
                if let Some(final_value) = debouncer.on_timer(now_ms) {
                    if let Some(ref proxy) = audio_proxy {
                        if let Err(e) = proxy.set_volume(final_value).await {
                            warn!("Failed to dispatch debounced volume to daemon: {e}");
                        }
                    }
                }
                if debouncer.is_timer_active() {
                    timer_sleep.as_mut().reset(tokio::time::Instant::now() + Duration::from_millis(50));
                    timer_active = true;
                }
            }

            // Signal from kynex-daemon on /org/kynexos/Daemon/Audio
            Some(Ok(signal)) = async {
                if let Some(ref mut s) = signal_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                let header = signal.header();
                let member = header.member();
                let member_str = member.map_or("", zbus::names::MemberName::as_str);
                if member_str == "PropertiesChanged" || member_str == "Changed" {
                    debug!("AudioModel received signal: {member_str}");
                    if let Some(ref proxy) = audio_proxy {
                        let state = fetch_audio_state(proxy).await;
                        dispatch_state(&qt_thread, &state);
        crate::debug::get_debug_hub()
            .update_model_readiness(|m| m.audio = true)
            .await;
                    }
                }
            }

            // Daemon appeared or disappeared
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
                        info!("kynex-daemon disconnected; resetting audio state");
                        audio_proxy = None;
                        dispatch_state(&qt_thread, &AudioState::default());
                    } else {
                        info!("kynex-daemon connected; fetching audio state");
                        audio_proxy = Audio1Proxy::builder(&session_conn)
                            .cache_properties(zbus::proxy::CacheProperties::No)
                            .build()
                            .await
                            .ok();
                        if let Some(ref proxy) = audio_proxy {
                            let state = fetch_audio_state(proxy).await;
                            dispatch_state(&qt_thread, &state);
        crate::debug::get_debug_hub()
            .update_model_readiness(|m| m.audio = true)
            .await;
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
    fn test_current_monotonic_ms() {
        let t1 = current_monotonic_ms();
        std::thread::sleep(Duration::from_millis(5));
        let t2 = current_monotonic_ms();
        assert!(t2 >= t1);
    }

    #[test]
    fn test_audio_command_variants() {
        let cmd1 = AudioCommand::SetVolume(80);
        let cmd2 = AudioCommand::SetMuted(true);
        assert_eq!(cmd1, AudioCommand::SetVolume(80));
        assert_eq!(cmd2, AudioCommand::SetMuted(true));
        assert_ne!(cmd1, AudioCommand::SetVolume(50));
    }

    #[test]
    fn test_audio_model_rust_default() {
        let rust = AudioModelRust::default();
        assert!(!rust.present);
        assert_eq!(rust.volume, 0);
        assert!(!rust.muted);
        assert!(rust.cmd_tx.is_none());
    }
}
