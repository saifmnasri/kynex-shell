//! Core pure-Rust library for system tray (StatusNotifierItem / StatusNotifierWatcher)
//! protocol logic, sanitization, validation, and registry management.
//!
//! Provides defense-in-depth against untrusted tray items:
//! - Hard resource limits (max 4 items per sender, max 64 items total).
//! - Sender verification preventing registration spoofing.
//! - Checked arithmetic validation of raw icon pixmaps (`IconPixmap`), bounding sizes to <= 64x64 and <= 64 KiB.
//! - Conversion from network byte order ARGB32 to RGBA.
//! - String sanitization: drops ASCII control characters and Unicode bidi overrides/isolates, strips XML/HTML markup tags.
//! - Signal-driven property refresh rate limiting (10 refreshes/second).
//! - Pure Rust without Qt or D-Bus dependencies.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::doc_markdown, clippy::chunks_exact_to_as_chunks)]

pub mod error;
pub mod limits;
pub mod pixmap;
pub mod rate_limit;
pub mod registry;
pub mod sanitize;
pub mod status;

pub use error::TrayError;
pub use limits::*;
pub use pixmap::{validate_and_convert_pixmap, ValidatedPixmap};
pub use rate_limit::RefreshRateLimiter;
pub use registry::{ItemKey, ItemState, TrayRegistry};
pub use sanitize::{
    sanitize_icon_name, sanitize_id, sanitize_text, sanitize_title, sanitize_tooltip_text,
    sanitize_tooltip_title,
};
pub use status::ItemStatus;
