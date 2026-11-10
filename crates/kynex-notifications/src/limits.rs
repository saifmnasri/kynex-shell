//! Standard limits and capacity thresholds for the desktop notifications subsystem.

/// Maximum allowed length of client `app_name` in bytes.
pub const MAX_APP_NAME_BYTES: usize = 64;

/// Maximum allowed length of notification `summary` in bytes.
pub const MAX_SUMMARY_BYTES: usize = 256;

/// Maximum allowed length of notification `body` in bytes.
pub const MAX_BODY_BYTES: usize = 4096;

/// Maximum number of action pairs (key, label) permitted per notification.
pub const MAX_ACTIONS: usize = 8;

/// Maximum character length of an action identifier key.
pub const MAX_ACTION_KEY_LEN: usize = 32;

/// Maximum allowed length of an action label in bytes.
pub const MAX_ACTION_LABEL_BYTES: usize = 64;

/// Maximum number of hint keys permitted per notification.
pub const MAX_HINTS: usize = 32;

/// Maximum number of retained notifications in the history ring buffer.
pub const MAX_HISTORY_ENTRIES: usize = 200;

/// Maximum number of simultaneously displayed popups on the desktop surface.
pub const MAX_VISIBLE_POPUPS: usize = 5;

/// Default expiration timeout in milliseconds when -1 is passed.
pub const DEFAULT_EXPIRE_TIMEOUT_MS: i32 = 5000;

/// Maximum permitted expiration timeout in milliseconds (capped at 24 hours).
pub const MAX_EXPIRE_TIMEOUT_MS: i32 = 86_400_000;

/// Maximum permitted width for embedded raw `image-data`.
pub const MAX_IMAGE_WIDTH: i32 = 256;

/// Maximum permitted height for embedded raw `image-data`.
pub const MAX_IMAGE_HEIGHT: i32 = 256;

/// Maximum permitted total byte payload for embedded raw `image-data` (256 KiB).
pub const MAX_IMAGE_BYTES: usize = 256 * 1024;

/// Burst capacity for the per-sender token-bucket rate limiter.
pub const RATE_LIMIT_BURST: f64 = 10.0;

/// Sustained rate in tokens per second for the per-sender rate limiter.
pub const RATE_LIMIT_SUSTAINED_PER_SEC: f64 = 2.0;

/// Maximum number of unique senders tracked simultaneously in the rate limiter.
pub const RATE_LIMIT_MAX_TRACKED_SENDERS: usize = 256;
