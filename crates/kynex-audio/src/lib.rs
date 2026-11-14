//! Pure-Rust core crate for audio backend parsing, limit enforcement, and event coalescing.
//!
//! Provides parsing for `pactl` JSON data, `pactl subscribe` line streams with bounded 4 KiB buffers,
//! multi-channel volume calculation using checked arithmetic mean, and hostile character sanitization.
//!
//! Enforces `#![forbid(unsafe_code)]` with zero unsafe, zero unwrap, and zero expect.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod coalescing;
pub mod error;
pub mod limits;
pub mod parser;
pub mod sanitize;
pub mod types;

pub use coalescing::{CoalesceAction, EventCoalescer};
pub use error::AudioError;
pub use limits::*;
pub use parser::{
    compute_master_volume, parse_server_info_json, parse_sinks_json, parse_subscribe_line,
    parse_volume_string, parse_volume_value, resolve_audio_status, BoundedLineReader,
};
pub use sanitize::sanitize_device_name;
pub use types::{AudioEvent, AudioStatus, ServerInfo, SinkInfo};
