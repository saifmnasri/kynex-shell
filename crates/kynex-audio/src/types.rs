//! Public data types for audio status, sink descriptors, and change events.

use serde::{Deserialize, Serialize};

/// Detailed descriptor for an individual audio sink device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SinkInfo {
    /// Internal index of the sink in the audio daemon.
    pub index: u32,
    /// System identifier string for the sink (e.g. `alsa_output...`).
    pub name: String,
    /// Sanitized human-readable device name or description.
    pub description: String,
    /// Whether the sink is currently muted.
    pub muted: bool,
    /// Master volume level clamped between 0 and 100.
    pub volume: u32,
    /// Individual channel volumes, each clamped between 0 and 100.
    pub channel_volumes: Vec<u32>,
}

/// Information returned by server introspection queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Identifier of the active default audio sink.
    pub default_sink_name: String,
}

/// Authoritative audio subsystem status published over D-Bus and consumed by UI models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AudioStatus {
    /// Whether an operational audio backend is present and active.
    pub present: bool,
    /// Master volume percentage clamped to [0, 100].
    pub volume: u32,
    /// Master mute state.
    pub muted: bool,
    /// Sanitized name of the active default audio output device.
    pub device_name: String,
}

/// Discrete event emitted on audio change notification streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioEvent {
    /// A sink property or state changed, optionally specifying the sink index.
    SinkChanged(Option<u32>),
    /// Server defaults changed (e.g. default sink switched).
    ServerChanged,
    /// Generic or untracked audio event.
    Other(String),
}
