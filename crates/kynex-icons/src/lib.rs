//! `kynex-icons`: UI-free Freedesktop Icon Theme Specification v0.13 lookup library.
//!
//! # Specification Compliance
//! Implements the Freedesktop.org Icon Theme Specification v0.13:
//! - Section "Directory Layout": Multi-root resolution across `$HOME/.icons`, `$XDG_DATA_HOME/icons`,
//!   `$XDG_DATA_DIRS/icons`, and `/usr/share/pixmaps`.
//! - Section "Theme Description File": Strict parser for `index.theme` supporting `[Icon Theme]`
//!   (`Name`, `Comment`, `Inherits`, `Directories`, `ScaledDirectories`) and per-directory sections
//!   (`Size`, `Scale`, `Context`, `Type`: `Fixed` | `Scalable` | `Threshold`, `MinSize`, `MaxSize`, `Threshold`).
//! - Section "Icon Lookup": Exact match pass (`DirectoryMatchesSize`), closest-distance pass
//!   (`DirectorySizeDistance`), parent theme inheritance with cycle detection and depth cap (8),
//!   mandatory `hicolor` terminal fallback, and unthemed `/usr/share/pixmaps` fallback.
//!
//! # Security & Hard Bounds
//! - Icon names validated against `^[a-zA-Z0-9._-]+$` with length <= 128 chars.
//! - Rejection of path traversal (`..`), path separators (`/`, `\`), NUL bytes, and control characters.
//! - Non-regular files (FIFOs, device nodes, sockets, directories) strictly rejected.
//! - Maximum file size capped at 1 MiB (`MAX_ICON_FILE_SIZE`).
//! - Symlinks resolved strictly inside canonical search roots.
//! - Absolute path icons from `.desktop` files accepted only inside icon roots or XDG data dirs.
//! - Zero `unwrap()`, zero `expect()`, zero `unsafe`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]

pub mod cache;
pub mod config;
pub mod error;
pub mod lookup;
pub mod roots;
pub mod theme;

pub use cache::{IconCache, LookupKey, DEFAULT_CACHE_CAPACITY};
pub use config::{read_configured_theme, resolve_active_theme, HICOLOR_THEME_NAME};
pub use error::IconError;
pub use lookup::{
    normalize_icon_name, validate_icon_name, IconResolver, MAX_ICON_NAME_LENGTH,
    MAX_INHERITANCE_DEPTH,
};
pub use roots::{IconRoots, MAX_ICON_FILE_SIZE};
pub use theme::{
    DirectoryType, SubdirectoryMeta, ThemeIndex, MAX_THEME_FILE_SIZE, MAX_THEME_LINES,
    MAX_THEME_LINE_LENGTH,
};

use std::path::PathBuf;
use std::sync::OnceLock;

static GLOBAL_RESOLVER: OnceLock<IconResolver> = OnceLock::new();

/// Convenience lookup using the ambient system environment roots and default bounded cache.
///
/// # Errors
/// Returns `IconError` if the icon name is invalid or lookup invariants fail.
pub fn lookup_icon(
    icon_name: &str,
    size: u32,
    scale: u32,
    theme_name: Option<&str>,
) -> Result<Option<PathBuf>, IconError> {
    let resolver = GLOBAL_RESOLVER.get_or_init(IconResolver::default);
    let theme = match theme_name {
        Some(t) => t.to_string(),
        None => resolve_active_theme(resolver.roots()),
    };
    resolver.find_icon(icon_name, size, scale, &theme)
}
