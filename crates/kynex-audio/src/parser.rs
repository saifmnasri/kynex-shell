//! Parser and validator for audio backend output, JSON data, and event streams.

use serde_json::Value;

use crate::error::AudioError;
use crate::limits::{MAX_DEVICES, MAX_LINE_LENGTH, MAX_VOLUME, MIN_VOLUME};
use crate::sanitize::sanitize_device_name;
use crate::types::{AudioEvent, AudioStatus, ServerInfo, SinkInfo};

/// Parses server information from JSON output produced by `pactl --format=json info`.
///
/// # Errors
///
/// Returns [`AudioError::JsonError`] if the payload is invalid JSON or lacks a valid default sink name.
pub fn parse_server_info_json(json_str: &str) -> Result<ServerInfo, AudioError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| AudioError::JsonError(format!("Invalid JSON in server info: {e}")))?;

    let default_sink = value
        .get("default_sink_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    Ok(ServerInfo {
        default_sink_name: default_sink,
    })
}

/// Parses channel volume percentage safely, clamping between [`MIN_VOLUME`] and [`MAX_VOLUME`].
///
/// Handles string percentages (e.g. `"45%"`), numeric values clamped to [0, 100],
/// negative numbers, oversized numbers, and NaN-like string inputs without panicking.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn parse_volume_value(val: &Value) -> u32 {
    match val {
        Value::Number(n) => {
            if let Some(v) = n.as_i64() {
                if v <= 0 {
                    MIN_VOLUME
                } else if v > 100 {
                    MAX_VOLUME
                } else {
                    u32::try_from(v).unwrap_or(MIN_VOLUME)
                }
            } else if let Some(v) = n.as_u64() {
                if v > 100 {
                    MAX_VOLUME
                } else {
                    u32::try_from(v).unwrap_or(MIN_VOLUME)
                }
            } else if let Some(f) = n.as_f64() {
                if f.is_nan() || f <= 0.0 {
                    MIN_VOLUME
                } else if f >= 100.0 {
                    MAX_VOLUME
                } else {
                    let rounded = f.round();
                    if rounded <= 0.0 {
                        MIN_VOLUME
                    } else if rounded >= 100.0 {
                        MAX_VOLUME
                    } else {
                        u32::try_from(rounded as i64).unwrap_or(MIN_VOLUME)
                    }
                }
            } else {
                MIN_VOLUME
            }
        }
        Value::String(s) => parse_volume_string(s),
        Value::Object(map) => {
            // Check for "value_percent" string first (e.g. "50%")
            if let Some(pct) = map.get("value_percent").and_then(Value::as_str) {
                parse_volume_string(pct)
            } else if let Some(raw) = map.get("value") {
                // If raw PulseAudio integer scale (65536 = 100%)
                if let Some(v) = raw.as_u64() {
                    if v > 100 && v <= 65536 {
                        let scaled = (v.saturating_mul(100)).checked_div(65536).unwrap_or(0);
                        u32::try_from(scaled)
                            .unwrap_or(MAX_VOLUME)
                            .clamp(MIN_VOLUME, MAX_VOLUME)
                    } else {
                        parse_volume_value(raw)
                    }
                } else {
                    parse_volume_value(raw)
                }
            } else {
                MIN_VOLUME
            }
        }
        _ => MIN_VOLUME,
    }
}

/// Parses a volume string safely, handling `%` suffixes and invalid numeric tokens.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn parse_volume_string(s: &str) -> u32 {
    let clean = s.trim().trim_end_matches('%').trim();
    if clean.eq_ignore_ascii_case("nan")
        || clean.eq_ignore_ascii_case("inf")
        || clean.eq_ignore_ascii_case("-inf")
    {
        return MIN_VOLUME;
    }

    if let Ok(val) = clean.parse::<i64>() {
        if val <= 0 {
            MIN_VOLUME
        } else if val > 100 {
            MAX_VOLUME
        } else {
            u32::try_from(val).unwrap_or(MIN_VOLUME)
        }
    } else if let Ok(f) = clean.parse::<f64>() {
        if f.is_nan() || f <= 0.0 {
            MIN_VOLUME
        } else if f >= 100.0 {
            MAX_VOLUME
        } else {
            let rounded = f.round();
            if rounded <= 0.0 {
                MIN_VOLUME
            } else if rounded >= 100.0 {
                MAX_VOLUME
            } else {
                u32::try_from(rounded as i64).unwrap_or(MIN_VOLUME)
            }
        }
    } else {
        MIN_VOLUME
    }
}

/// Combines multiple channel volumes into a single master volume using checked arithmetic mean.
///
/// Clamps result strictly to [0, 100]. Returns 0 if channel list is empty.
#[must_use]
pub fn compute_master_volume(channels: &[u32]) -> u32 {
    if channels.is_empty() {
        return MIN_VOLUME;
    }

    let count = u32::try_from(channels.len()).unwrap_or(1);
    let mut sum: u32 = 0;
    for &ch in channels {
        let clamped = ch.clamp(MIN_VOLUME, MAX_VOLUME);
        sum = sum.saturating_add(clamped);
    }

    // Arithmetic mean with rounding to nearest: (sum + count / 2) / count
    let half_count = count.checked_div(2).unwrap_or(0);
    let rounded_sum = sum.saturating_add(half_count);
    let avg = rounded_sum.checked_div(count).unwrap_or(0);

    avg.clamp(MIN_VOLUME, MAX_VOLUME)
}

/// Parses the array of sink descriptions from JSON output produced by `pactl --format=json list sinks`.
///
/// Enforces the hard limit of at most [`MAX_DEVICES`] (64). Sinks beyond this limit are discarded.
///
/// # Errors
///
/// Returns [`AudioError::JsonError`] if the payload is not valid JSON or not an array.
pub fn parse_sinks_json(json_str: &str) -> Result<Vec<SinkInfo>, AudioError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| AudioError::JsonError(format!("Invalid JSON in sinks list: {e}")))?;

    let array = value
        .as_array()
        .ok_or_else(|| AudioError::JsonError("Expected JSON array for sinks list".to_string()))?;

    let mut sinks = Vec::with_capacity(array.len().min(MAX_DEVICES));

    for item in array.iter().take(MAX_DEVICES) {
        let raw_idx = item.get("index").and_then(Value::as_u64).unwrap_or(0);
        let index = u32::try_from(raw_idx).unwrap_or(0);

        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let raw_desc = item
            .get("description")
            .and_then(Value::as_str)
            .or_else(|| {
                item.get("properties")
                    .and_then(|p| p.get("device.description"))
                    .and_then(Value::as_str)
            })
            .unwrap_or(&name);

        let sanitized_desc = sanitize_device_name(raw_desc);

        let muted = match item.get("mute") {
            Some(Value::Bool(b)) => *b,
            Some(Value::String(s)) => {
                s.eq_ignore_ascii_case("yes") || s.eq_ignore_ascii_case("true")
            }
            _ => false,
        };

        let mut channel_volumes = Vec::new();
        if let Some(vol_obj) = item.get("volume").and_then(Value::as_object) {
            for (_ch_name, ch_val) in vol_obj {
                channel_volumes.push(parse_volume_value(ch_val));
            }
        } else if let Some(vol_val) = item.get("volume") {
            channel_volumes.push(parse_volume_value(vol_val));
        }

        let volume = compute_master_volume(&channel_volumes);

        sinks.push(SinkInfo {
            index,
            name,
            description: sanitized_desc,
            muted,
            volume,
            channel_volumes,
        });
    }

    Ok(sinks)
}

/// Resolves overall audio status by matching default sink name with discovered sinks.
#[must_use]
pub fn resolve_audio_status(server: &ServerInfo, sinks: &[SinkInfo]) -> AudioStatus {
    if sinks.is_empty() {
        return AudioStatus {
            present: false,
            volume: 0,
            muted: false,
            device_name: String::new(),
        };
    }

    let target_sink = sinks
        .iter()
        .find(|s| !server.default_sink_name.is_empty() && s.name == server.default_sink_name)
        .or_else(|| sinks.first());

    match target_sink {
        Some(sink) => AudioStatus {
            present: true,
            volume: sink.volume,
            muted: sink.muted,
            device_name: sink.description.clone(),
        },
        None => AudioStatus {
            present: true,
            volume: 0,
            muted: false,
            device_name: String::new(),
        },
    }
}

/// Parses a single line from `pactl subscribe`.
#[must_use]
pub fn parse_subscribe_line(line: &str) -> Option<AudioEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Typical subscribe format:
    // Event 'change' on sink #55
    // Event 'new' on sink #56
    // Event 'remove' on sink #55
    // Event 'change' on server #0
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() >= 4 && parts[0] == "Event" && parts[2] == "on" {
        let facility = parts[3];
        if facility == "sink" {
            let index = parts.get(4).and_then(|idx_str| {
                idx_str
                    .strip_prefix('#')
                    .and_then(|s| s.parse::<u32>().ok())
            });
            return Some(AudioEvent::SinkChanged(index));
        } else if facility == "server" {
            return Some(AudioEvent::ServerChanged);
        }
    }

    Some(AudioEvent::Other(trimmed.to_string()))
}

/// Bounded streaming line parser that prevents memory exhaustion from unbounded lines.
///
/// Enforces a hard line length limit of [`MAX_LINE_LENGTH`] (4096 bytes).
/// If input data exceeds this buffer without a newline character, the buffer is purged
/// and [`AudioError::LineTooLong`] is yielded, discarding excess data until the next newline.
#[derive(Debug, Default)]
pub struct BoundedLineReader {
    buffer: Vec<u8>,
    in_overflow: bool,
}

impl BoundedLineReader {
    /// Creates a new bounded line reader with fixed initial capacity.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(MAX_LINE_LENGTH),
            in_overflow: false,
        }
    }

    /// Feeds a chunk of bytes and extracts completed lines.
    ///
    /// # Errors
    ///
    /// Yields [`AudioError::LineTooLong`] if a line exceeds [`MAX_LINE_LENGTH`].
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Result<String, AudioError>> {
        let mut results = Vec::new();

        for &byte in chunk {
            if byte == b'\n' {
                if self.in_overflow {
                    self.in_overflow = false;
                    self.buffer.clear();
                    results.push(Err(AudioError::LineTooLong(MAX_LINE_LENGTH)));
                } else {
                    let line = String::from_utf8_lossy(&self.buffer).to_string();
                    self.buffer.clear();
                    results.push(Ok(line));
                }
            } else if self.in_overflow {
                // Drop excess bytes until newline
            } else if self.buffer.len() >= MAX_LINE_LENGTH {
                // Buffer filled without seeing a newline
                self.in_overflow = true;
                self.buffer.clear();
            } else {
                self.buffer.push(byte);
            }
        }

        results
    }

    /// Flushes any remaining bytes at stream EOF.
    #[must_use]
    pub fn flush(&mut self) -> Option<Result<String, AudioError>> {
        if self.in_overflow {
            self.in_overflow = false;
            self.buffer.clear();
            Some(Err(AudioError::LineTooLong(MAX_LINE_LENGTH)))
        } else if !self.buffer.is_empty() {
            let line = String::from_utf8_lossy(&self.buffer).to_string();
            self.buffer.clear();
            Some(Ok(line))
        } else {
            None
        }
    }
}
