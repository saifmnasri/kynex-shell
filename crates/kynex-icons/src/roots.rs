//! Base search root discovery and symlink boundary confinement.
//!
//! Freedesktop Icon Theme Specification v0.13: Section "Directory Layout".
//! Applications look in:
//! 1. `$HOME/.icons` (legacy backwards compatibility)
//! 2. `$XDG_DATA_HOME/icons` (`$HOME/.local/share/icons`)
//! 3. Each `<dir>/icons` directory in `$XDG_DATA_DIRS` (`/usr/local/share/icons`, `/usr/share/icons`)
//! 4. `/usr/share/pixmaps` (unthemed legacy icons)

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::IconError;

/// Maximum allowed icon file size (1 MiB).
pub const MAX_ICON_FILE_SIZE: u64 = 1024 * 1024;

/// Discovered search roots for icon themes and fallback pixmaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconRoots {
    /// Ordered list of icon theme root directories.
    pub theme_roots: Vec<PathBuf>,
    /// Fallback unthemed pixmaps directories (e.g. `/usr/share/pixmaps`).
    pub pixmap_roots: Vec<PathBuf>,
    /// Allowed XDG data roots for validating absolute-path icons from `.desktop` files.
    pub xdg_data_roots: Vec<PathBuf>,
    /// Canonicalized roots for symlink containment checks.
    pub canonical_allowed_roots: Vec<PathBuf>,
}

impl IconRoots {
    /// Discover icon roots from ambient environment variables (`HOME`, `XDG_DATA_HOME`, `XDG_DATA_DIRS`).
    #[must_use]
    pub fn from_env() -> Self {
        let home = env::var_os("HOME").map(PathBuf::from);

        let xdg_data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".local/share")));

        let xdg_data_dirs = env::var_os("XDG_DATA_DIRS").map_or_else(
            || {
                vec![
                    PathBuf::from("/usr/local/share"),
                    PathBuf::from("/usr/share"),
                ]
            },
            |dirs| env::split_paths(&dirs).collect::<Vec<PathBuf>>(),
        );

        let mut theme_roots = Vec::new();

        // 1. $HOME/.icons (legacy backwards compatibility per Section "Directory Layout")
        if let Some(ref h) = home {
            let legacy = h.join(".icons");
            if legacy.exists() {
                theme_roots.push(legacy);
            }
        }

        // 2. $XDG_DATA_HOME/icons
        if let Some(ref data_home) = xdg_data_home {
            let user_icons = data_home.join("icons");
            if user_icons.exists() {
                theme_roots.push(user_icons);
            }
        }

        // 3. $XDG_DATA_DIRS/icons
        for dir in &xdg_data_dirs {
            let sys_icons = dir.join("icons");
            if sys_icons.exists() && !theme_roots.contains(&sys_icons) {
                theme_roots.push(sys_icons);
            }
        }

        // 4. Fallback unthemed pixmaps directory (/usr/share/pixmaps)
        let mut pixmap_roots = Vec::new();
        let default_pixmaps = PathBuf::from("/usr/share/pixmaps");
        if default_pixmaps.exists() {
            pixmap_roots.push(default_pixmaps);
        }

        let mut xdg_data_roots = Vec::new();
        if let Some(data_home) = xdg_data_home {
            if data_home.exists() {
                xdg_data_roots.push(data_home);
            }
        }
        for dir in xdg_data_dirs {
            if dir.exists() && !xdg_data_roots.contains(&dir) {
                xdg_data_roots.push(dir);
            }
        }

        let mut canonical_allowed_roots = Vec::new();
        for root in theme_roots
            .iter()
            .chain(pixmap_roots.iter())
            .chain(xdg_data_roots.iter())
        {
            if let Ok(canon) = fs::canonicalize(root) {
                if !canonical_allowed_roots.contains(&canon) {
                    canonical_allowed_roots.push(canon);
                }
            }
        }

        Self {
            theme_roots,
            pixmap_roots,
            xdg_data_roots,
            canonical_allowed_roots,
        }
    }

    /// Construct custom roots (primarily for testing with isolated temporary trees).
    #[must_use]
    pub fn custom(
        theme_roots: Vec<PathBuf>,
        pixmap_roots: Vec<PathBuf>,
        xdg_data_roots: Vec<PathBuf>,
    ) -> Self {
        let mut canonical_allowed_roots = Vec::new();
        for root in theme_roots
            .iter()
            .chain(pixmap_roots.iter())
            .chain(xdg_data_roots.iter())
        {
            if let Ok(canon) = fs::canonicalize(root) {
                if !canonical_allowed_roots.contains(&canon) {
                    canonical_allowed_roots.push(canon);
                }
            }
        }
        Self {
            theme_roots,
            pixmap_roots,
            xdg_data_roots,
            canonical_allowed_roots,
        }
    }

    /// Base theme search directories.
    #[must_use]
    pub fn theme_roots(&self) -> &[PathBuf] {
        &self.theme_roots
    }

    /// Fallback pixmap directories.
    #[must_use]
    pub fn pixmap_roots(&self) -> &[PathBuf] {
        &self.pixmap_roots
    }

    /// Check whether a canonical path resides within any of the allowed canonical roots.
    #[must_use]
    pub fn is_confined(&self, canonical_target: &Path) -> bool {
        self.canonical_allowed_roots
            .iter()
            .any(|root| canonical_target.starts_with(root))
    }

    /// Validate and resolve a file path, ensuring:
    /// - The target exists and is a regular file (not FIFO, socket, device, directory).
    /// - File size does not exceed `MAX_ICON_FILE_SIZE` (1 MiB).
    /// - Symlinks resolve strictly inside one of the allowed roots.
    ///
    /// # Errors
    /// Returns `IconError` if the file is missing, not a regular file, exceeds 1 MiB, or escapes roots.
    pub fn validate_and_resolve_file(&self, path: &Path) -> Result<PathBuf, IconError> {
        let symlink_meta = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(IconError::FileNotFound(path.to_path_buf()));
            }
            Err(e) => return Err(IconError::Io(e)),
        };

        let canonical = match fs::canonicalize(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(IconError::FileNotFound(path.to_path_buf()));
            }
            Err(e) => return Err(IconError::Io(e)),
        };

        if !self.is_confined(&canonical) {
            return Err(IconError::SymlinkEscapesRoot {
                symlink: path.to_path_buf(),
                target: canonical,
            });
        }

        let target_meta = match fs::metadata(&canonical) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(IconError::FileNotFound(canonical));
            }
            Err(e) => return Err(IconError::Io(e)),
        };

        if !target_meta.is_file() {
            return Err(IconError::NotARegularFile(canonical));
        }

        let size = target_meta.len();
        if size > MAX_ICON_FILE_SIZE {
            return Err(IconError::FileSizeExceeded {
                path: canonical,
                size,
                limit: MAX_ICON_FILE_SIZE,
            });
        }

        // Suppress unused warning on symlink_meta in case it's only inspected for regular checks
        let _ = symlink_meta;

        Ok(canonical)
    }

    /// Validate an absolute-path icon specified in a `.desktop` file.
    ///
    /// Must:
    /// - Be an absolute path starting with `/`.
    /// - End with `.png` or `.svg`.
    /// - Be a regular file <= 1 MiB.
    /// - Reside inside an allowed icon root or XDG data directory.
    ///
    /// # Errors
    /// Returns `IconError` if the path is relative, unsupported extension, exceeds 1 MiB, or is unconfined.
    pub fn validate_absolute_path_icon(&self, path: &Path) -> Result<PathBuf, IconError> {
        if !path.is_absolute() {
            return Err(IconError::AbsolutePathNotAllowed(path.to_path_buf()));
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if ext != "png" && ext != "svg" {
            return Err(IconError::InvalidIconName {
                name: path.to_string_lossy().to_string(),
                reason: "absolute path icon must have .png or .svg extension",
            });
        }

        self.validate_and_resolve_file(path).map_err(|e| match e {
            IconError::SymlinkEscapesRoot { symlink, .. } => {
                IconError::AbsolutePathNotAllowed(symlink)
            }
            other => other,
        })
    }
}
