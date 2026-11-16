//! Definitions and client proxy for `org.kynexos.Daemon.Audio1` D-Bus interface.

/// The interface name for the audio management service.
pub const AUDIO_INTERFACE: &str = "org.kynexos.Daemon.Audio1";

/// Canonical D-Bus object path for audio management.
pub const AUDIO_PATH: &str = "/org/kynexos/Daemon/Audio";

/// Global rate limit for audio mutations (20 calls/second).
pub const AUDIO_MUTATION_RATE_LIMIT_PER_SEC: u32 = 20;

/// Client-side D-Bus proxy for `org.kynexos.Daemon.Audio1`.
#[zbus::proxy(
    interface = "org.kynexos.Daemon.Audio1",
    default_service = "org.kynexos.Daemon",
    default_path = "/org/kynexos/Daemon/Audio"
)]
pub trait Audio1 {
    /// Whether an operational audio backend is present and active.
    #[zbus(property)]
    fn present(&self) -> zbus::Result<bool>;

    /// Master output volume percentage clamped to [0, 100].
    #[zbus(property)]
    fn volume(&self) -> zbus::Result<u32>;

    /// Master mute state.
    #[zbus(property)]
    fn muted(&self) -> zbus::Result<bool>;

    /// Sanitized name of the active default audio output device.
    #[zbus(property)]
    fn device_name(&self) -> zbus::Result<String>;

    /// Sets the master output volume level (0-100).
    fn set_volume(&self, volume: u32) -> zbus::Result<()>;

    /// Sets the master output mute state.
    fn set_muted(&self, muted: bool) -> zbus::Result<()>;
}
