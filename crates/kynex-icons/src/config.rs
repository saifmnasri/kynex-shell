//! Configuration loading and deterministic theme selection policy.
//!
//! Enforces:
//! - User overrides in `$XDG_CONFIG_HOME/kynex/config.toml` (`[icons] theme = "..."`).
//! - System default discovery via `/usr/share/icons/default/index.theme`.
//! - Mandatory base fallback to `"hicolor"`.
//! - Strict compliance with Rule 08: ZERO hardcoded third-party brand/product names.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::roots::IconRoots;

/// The standard Freedesktop fallback icon theme.
pub const HICOLOR_THEME_NAME: &str = "hicolor";

#[derive(Debug, Deserialize, Default)]
struct KynexConfig {
    icons: Option<IconsConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct IconsConfig {
    theme: Option<String>,
}

/// Read the user-configured theme name from `$XDG_CONFIG_HOME/kynex/config.toml`.
#[must_use]
pub fn read_configured_theme() -> Option<String> {
    let config_dir = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;

    let config_file = config_dir.join("kynex/config.toml");
    read_theme_from_path(&config_file)
}

/// Read theme name from a specific `config.toml` path.
#[must_use]
pub fn read_theme_from_path(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }

    // Limit config read to 64 KiB
    let content = match fs::read_to_string(path) {
        Ok(c) if c.len() <= 64 * 1024 => c,
        _ => return None,
    };

    let parsed: KynexConfig = toml::from_str(&content).ok()?;
    let theme = parsed.icons?.theme?.trim().to_string();
    if theme.is_empty() {
        None
    } else {
        Some(theme)
    }
}

/// Check whether a theme directory with `index.theme` exists within the search roots.
#[must_use]
pub fn theme_exists(theme_name: &str, roots: &IconRoots) -> bool {
    for root in &roots.theme_roots {
        let theme_index = root.join(theme_name).join("index.theme");
        if theme_index.is_file() {
            return true;
        }
    }
    false
}

/// Resolve the active theme name deterministically:
/// 1. User configuration (`~/.config/kynex/config.toml` `[icons].theme`).
/// 2. System default symlink/file (`/usr/share/icons/default/index.theme` `Inherits`).
/// 3. Standard Freedesktop fallback (`"hicolor"`).
#[must_use]
pub fn resolve_active_theme(roots: &IconRoots) -> String {
    // 1. User config
    if let Some(user_theme) = read_configured_theme() {
        if theme_exists(&user_theme, roots) {
            return user_theme;
        }
    }

    // 2. System default (/usr/share/icons/default/index.theme)
    for root in &roots.theme_roots {
        let default_theme_path = root.join("default/index.theme");
        if default_theme_path.is_file() {
            if let Ok(content) = fs::read_to_string(&default_theme_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(inherits) = trimmed.strip_prefix("Inherits=") {
                        for candidate in inherits.split(',') {
                            let candidate = candidate.trim();
                            if !candidate.is_empty() && theme_exists(candidate, roots) {
                                return candidate.to_string();
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Fallback to standard spec theme
    HICOLOR_THEME_NAME.to_string()
}
