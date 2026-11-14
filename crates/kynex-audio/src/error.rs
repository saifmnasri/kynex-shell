//! Error types for the audio subsystem.

use thiserror::Error;

/// Errors occurring during audio parsing, backend communication, or limit validation.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum AudioError {
    /// A single line exceeded the maximum bounded length of 4 KiB.
    #[error("Output line exceeded maximum limit of {0} bytes")]
    LineTooLong(usize),

    /// Deserialization or JSON structure validation failed.
    #[error("Failed to parse JSON audio backend payload: {0}")]
    JsonError(String),

    /// Text parser encountered invalid or unparsable syntax.
    #[error("Failed to parse audio backend text: {0}")]
    ParseError(String),

    /// The number of discovered devices exceeded the device cap.
    #[error("Device count exceeded maximum capacity of {0}")]
    DeviceLimitExceeded(usize),

    /// An I/O error occurred during subprocess interaction or streaming.
    #[error("I/O error during audio backend operation: {0}")]
    Io(String),

    /// Subprocess exited with a failure code or abnormal termination.
    #[error("Subprocess execution error: {0}")]
    ProcessError(String),

    /// Operation timed out.
    #[error("Audio backend operation timed out after 2 seconds")]
    Timeout,

    /// No operational audio backend found.
    #[error("Audio backend not found or unavailable")]
    MissingBackend,
}
