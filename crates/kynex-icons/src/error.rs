//! Error types for icon resolution and theme parsing.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during icon resolution or theme index parsing.
#[derive(Debug, Error)]
pub enum IconError {
    /// The requested icon name violates charset, length, or path bounds.
    #[error("invalid icon name '{name}': {reason}")]
    InvalidIconName {
        /// The invalid icon name.
        name: String,
        /// Reason for rejection.
        reason: &'static str,
    },

    /// The theme description file (`index.theme`) is malformed or exceeds safety bounds.
    #[error("invalid theme file at '{path}': {reason}")]
    InvalidThemeFile {
        /// Path to the theme file.
        path: PathBuf,
        /// Reason for rejection.
        reason: String,
    },

    /// The icon or theme file was not found.
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    /// The target is not a regular file (e.g. FIFO, socket, device node, directory).
    #[error("not a regular file: {0}")]
    NotARegularFile(PathBuf),

    /// The icon file exceeds the maximum allowed size (1 MiB).
    #[error("file size of {path} ({size} bytes) exceeds maximum allowed size ({limit} bytes)")]
    FileSizeExceeded {
        /// Path to the oversized file.
        path: PathBuf,
        /// Actual file size.
        size: u64,
        /// Maximum allowed size.
        limit: u64,
    },

    /// A symlink resolved outside the allowed root directories.
    #[error("symlink '{symlink}' resolves outside allowed root to '{target}'")]
    SymlinkEscapesRoot {
        /// Symlink source path.
        symlink: PathBuf,
        /// Resolved target path.
        target: PathBuf,
    },

    /// An absolute path icon from a `.desktop` file is outside allowed icon roots or XDG data dirs.
    #[error("absolute path icon '{0}' is outside allowed search roots")]
    AbsolutePathNotAllowed(PathBuf),

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
