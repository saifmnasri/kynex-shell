//! Error types for system tray item registration, validation, and interaction.

use thiserror::Error;

/// Errors produced during tray item management and validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrayError {
    /// Registration argument does not resolve to the caller's own connection bus name.
    #[error("Sender mismatch: registration service does not resolve to caller bus name")]
    SenderMismatch,

    /// Maximum number of items per sender (4) has been exceeded.
    #[error("Sender limit exceeded: max {0} items per client")]
    SenderLimitExceeded(usize),

    /// Total system tray capacity (64 items) has been exceeded.
    #[error("Total limit exceeded: max {0} items total")]
    TotalLimitExceeded(usize),

    /// Item identifier not found in the registry.
    #[error("Item not found: {0}")]
    ItemNotFound(String),

    /// Object path is malformed or invalid.
    #[error("Invalid object path: {0}")]
    InvalidObjectPath(String),

    /// Icon pixmap payload failed validation.
    #[error("Invalid icon pixmap: {0}")]
    InvalidPixmap(String),

    /// Property refresh rate limit exceeded.
    #[error("Rate limit exceeded: too many updates per second")]
    RateLimitExceeded,

    /// Asynchronous operation timed out.
    #[error("Operation timed out")]
    Timeout,
}
