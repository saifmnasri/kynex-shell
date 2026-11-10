//! Pure Rust implementation of desktop notifications core state, sanitization,
//! rate limiting, and expiry scheduling.
//!
//! Licensed under GPL-3.0-or-later. Free of Qt and D-Bus runtime dependencies.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod image;
pub mod limits;
pub mod rate_limit;
pub mod sanitize;
pub mod state;

pub use error::NotificationError;
pub use image::{validate_image_data, RawImageData};
pub use limits::*;
pub use rate_limit::RateLimiter;
pub use sanitize::{
    sanitize_action_label, sanitize_app_name, sanitize_body, sanitize_summary, sanitize_text,
    truncate_to_char_boundary, validate_action_key, validate_and_sanitize_actions,
};
pub use state::{
    CloseOutcome, CloseReason, Notification, NotificationManager, NotifyOutcome, Urgency,
};
