//! Style manifest parsing and validation.

use crate::error::StyleError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current supported style manifest schema version.
pub const STYLE_SCHEMA_VERSION: u32 = 1;

/// Maximum allowed file size for style.toml (1 MiB).
pub const MAX_STYLE_FILE_SIZE: u64 = 1024 * 1024;

/// Style pack kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StyleKind {
    /// Full style pack providing layout, surfaces, and look.
    Full,
    /// Look style pack providing tokens, skins, and assets.
    Look,
}

/// Layout archetype description.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Archetype {
    /// Top bar with floating centered dock.
    Dock,
    /// Single bottom taskbar with launcher and system tray.
    Taskbar,
    /// Full-screen overview with workspaces and app grid.
    Overview,
    /// Custom layout.
    Custom,
}

/// The main style pack manifest representation (`style.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleManifest {
    /// Schema version of the style manifest.
    pub schema_version: u32,
    /// Unique identifier [a-z0-9-]+
    pub id: String,
    /// Display name.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Author / contributors.
    pub author: String,
    /// License identifier (e.g. MIT).
    pub license: String,
    /// Kind of style pack.
    pub kind: StyleKind,
    /// Archetype pattern.
    pub archetype: Archetype,
    /// Minimum required shell API version.
    pub min_shell_api: u32,
    /// Entry point configuration.
    pub entry: EntryConfig,
    /// Tokens configuration.
    pub tokens: TokensConfig,
    /// Optional `KWin` integration settings.
    pub kwin: Option<KWinConfig>,
    /// Optional assets directories.
    pub assets: Option<AssetsConfig>,
}

/// Entry point files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryConfig {
    /// Main QML entry point file (e.g. `Shell.qml`).
    pub shell: String,
}

/// Design tokens file reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokensConfig {
    /// Relative path to tokens.toml.
    pub file: String,
}

/// `KWin` configuration settings and theme directories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KWinConfig {
    /// Window decoration theme directory relative path.
    pub decoration: Option<String>,
    /// `KWin` effects configuration file relative path.
    pub effects: Option<String>,
}

/// Asset directory mappings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetsConfig {
    /// Icons directory relative path.
    pub icons: Option<String>,
    /// Cursors directory relative path.
    pub cursors: Option<String>,
    /// Sounds directory relative path.
    pub sounds: Option<String>,
    /// Wallpapers directory relative path.
    pub wallpapers: Option<String>,
}

impl StyleManifest {
    /// Validate manifest fields, IDs, and verify that referenced files are safe.
    ///
    /// # Errors
    ///
    /// Returns a [`StyleError`] if any field violates format, schema version, or file security requirements.
    pub fn validate(&self, manifest_path: &Path, pack_dir: &Path) -> Result<(), StyleError> {
        if self.schema_version != STYLE_SCHEMA_VERSION {
            return Err(StyleError::UnsupportedSchemaVersion {
                path: manifest_path.to_path_buf(),
                version: self.schema_version,
                expected: STYLE_SCHEMA_VERSION,
            });
        }

        Self::validate_id(&self.id)?;

        // Validate entry shell file
        Self::validate_rel_path("entry.shell", &self.entry.shell)?;
        Self::ensure_file_exists(pack_dir, &self.entry.shell)?;

        // Validate tokens file
        Self::validate_rel_path("tokens.file", &self.tokens.file)?;
        Self::ensure_file_exists(pack_dir, &self.tokens.file)?;

        // Validate optional kwin paths
        if let Some(ref kwin) = self.kwin {
            if let Some(ref deco) = kwin.decoration {
                Self::validate_rel_path("kwin.decoration", deco)?;
            }
            if let Some(ref effects) = kwin.effects {
                Self::validate_rel_path("kwin.effects", effects)?;
            }
        }

        // Validate optional asset paths
        if let Some(ref assets) = self.assets {
            if let Some(ref p) = assets.icons {
                Self::validate_rel_path("assets.icons", p)?;
            }
            if let Some(ref p) = assets.cursors {
                Self::validate_rel_path("assets.cursors", p)?;
            }
            if let Some(ref p) = assets.sounds {
                Self::validate_rel_path("assets.sounds", p)?;
            }
            if let Some(ref p) = assets.wallpapers {
                Self::validate_rel_path("assets.wallpapers", p)?;
            }
        }

        Ok(())
    }

    fn validate_id(id: &str) -> Result<(), StyleError> {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(StyleError::InvalidId { id: id.to_string() });
        }
        Ok(())
    }

    fn validate_rel_path(context: &str, path_str: &str) -> Result<(), StyleError> {
        let path = Path::new(path_str);
        if path.is_absolute() {
            return Err(StyleError::UnsafePath {
                context: context.to_string(),
                path: path_str.to_string(),
            });
        }

        for component in path.components() {
            match component {
                std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_) => {
                    return Err(StyleError::UnsafePath {
                        context: context.to_string(),
                        path: path_str.to_string(),
                    });
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn ensure_file_exists(pack_dir: &Path, rel_path: &str) -> Result<PathBuf, StyleError> {
        let full_path = pack_dir.join(rel_path);
        let canonical_pack = pack_dir.canonicalize().map_err(|e| StyleError::Io {
            path: pack_dir.to_path_buf(),
            source: e,
        })?;

        let canonical_target = full_path
            .canonicalize()
            .map_err(|_| StyleError::FileNotFound {
                path: full_path.clone(),
            })?;

        // Guard against symlink traversal out of the pack directory
        if !canonical_target.starts_with(&canonical_pack) {
            return Err(StyleError::UnsafePath {
                context: format!("symlink target for {rel_path}"),
                path: canonical_target.to_string_lossy().to_string(),
            });
        }

        Ok(canonical_target)
    }
}
