//! Hard limits and bounds for audio backend data parsing.

/// Maximum line length for audio backend output and subscribe streams (4 KiB).
pub const MAX_LINE_LENGTH: usize = 4096;

/// Maximum number of audio sink devices accepted from backend queries.
pub const MAX_DEVICES: usize = 64;

/// Maximum length in characters for sanitized audio device names/descriptions.
pub const MAX_DEVICE_NAME_CHARS: usize = 128;

/// Minimum allowable volume percentage.
pub const MIN_VOLUME: u32 = 0;

/// Maximum allowable volume percentage.
pub const MAX_VOLUME: u32 = 100;

/// Default coalescing debounce duration in milliseconds.
pub const DEFAULT_COALESCE_DEBOUNCE_MS: u64 = 25;

/// Maximum adaptive coalescing debounce duration in milliseconds (250 ms).
pub const MAX_COALESCE_WINDOW_MS: u64 = 250;

/// Inactivity interval after which the adaptive coalescing window resets to the minimum (1000 ms).
pub const QUIET_RESET_MS: u64 = 1000;
