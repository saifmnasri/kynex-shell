//! Error types for the style pack parser and validator.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when parsing or validating a style pack.
#[derive(Debug, Error)]
pub enum StyleError {
    /// File I/O error.
    #[error("I/O error reading {path}: {source}")]
    Io {
        /// The path where the I/O error occurred.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// File exceeds the maximum allowed size.
    #[error("File {path} exceeds maximum size of {max_bytes} bytes (found {found_bytes} bytes)")]
    FileTooLarge {
        /// The file path.
        path: PathBuf,
        /// Maximum allowed bytes.
        max_bytes: u64,
        /// Found bytes.
        found_bytes: u64,
    },

    /// File is not a regular file (e.g. FIFO, device, socket).
    #[error("File {path} is not a regular file")]
    NotARegularFile {
        /// The file path.
        path: PathBuf,
    },

    /// TOML deserialization error.
    #[error("TOML decode error in {path}: {source}")]
    TomlDecode {
        /// The file path.
        path: PathBuf,
        /// The underlying TOML error.
        #[source]
        source: toml::de::Error,
    },

    /// Unsupported schema version.
    #[error("Unsupported schema version {version} in {path} (expected {expected})")]
    UnsupportedSchemaVersion {
        /// The file path.
        path: PathBuf,
        /// The version encountered.
        version: u32,
        /// The expected version.
        expected: u32,
    },

    /// Path traversal or unsafe path detected.
    #[error("Unsafe path detected in {context}: '{path}'")]
    UnsafePath {
        /// Description of context where the unsafe path was found.
        context: String,
        /// The unsafe path string.
        path: String,
    },

    /// Invalid identifier format.
    #[error("Invalid style id '{id}': must match [a-z0-9-]+")]
    InvalidId {
        /// The invalid id.
        id: String,
    },

    /// Invalid color token value.
    #[error("Invalid hex color code '{color}' in tokens")]
    InvalidColor {
        /// The invalid color value.
        color: String,
    },

    /// Referenced file is missing.
    #[error("Required file '{path}' not found in style pack directory")]
    FileNotFound {
        /// The missing file path.
        path: PathBuf,
    },
}
