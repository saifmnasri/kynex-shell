//! Error types for the desktop notifications subsystem.

use thiserror::Error;

/// Errors produced during notification validation, rate limiting, or state processing.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum NotificationError {
    /// The caller or payload exceeded defined safety limits or rate bounds.
    #[error("Rate limit or payload bounds exceeded: {0}")]
    LimitsExceeded(&'static str),

    /// The caller attempted to mutate or close a notification owned by another D-Bus sender.
    #[error("Sender mismatch: caller cannot modify notification owned by another sender")]
    SenderMismatch,

    /// Raw image-data failed strict format, dimension, or rowstride validation.
    #[error("Invalid raw image-data: {0}")]
    InvalidImageData(&'static str),

    /// An input argument failed validation (e.g. invalid action key charset).
    #[error("Invalid argument: {0}")]
    InvalidArgs(&'static str),

    /// The requested notification ID was not found in active state.
    #[error("Notification with ID {0} not found")]
    NotFound(u32),
}
