//! Configuration loading and deterministic theme selection policy.
//!
//! Enforces:
//! - User overrides in `$XDG_CONFIG_HOME/kynex/config.toml` (`[icons] theme = "..."`).
//! - Desktop portal setting via `org.freedesktop.portal.Settings.Read("org.gnome.desktop.interface", "icon-theme")`.
//! - System default discovery via `/usr/share/icons/default/index.theme`.
//! - Mandatory base fallback to `"hicolor"`.
//! - Strict compliance with Rule 08: ZERO hardcoded third-party brand/product names.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// Async query to xdg-desktop-portal Settings interface.
pub async fn fetch_portal_icon_theme_async() -> Option<String> {
    let conn = zbus::Connection::session().await.ok()?;
    let proxy = zbus::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )
    .await
    .ok()?;

    let (val,): (zbus::OwnedVariant,) = proxy
        .call("Read", &("org.gnome.desktop.interface", "icon-theme"))
        .await
        .ok()?;

    let inner = val.downcast_ref::<zbus::OwnedVariant>().ok()?;
    let theme_str = inner.downcast_ref::<String>().ok()?;
    let trimmed = theme_str.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Directly queries the portal for the icon theme setting with a 100ms timeout.
#[must_use]
pub fn fetch_portal_icon_theme() -> Option<String> {
    // Run on a separate thread with an isolated runtime to prevent nested block_on panics.
    std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        rt.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                fetch_portal_icon_theme_async(),
            )
            .await
            .ok()
            .flatten()
        })
    })
    .join()
    .ok()
    .flatten()
}

/// Read active icon theme from xdg-desktop-portal Settings interface (cached).
#[must_use]
pub fn read_portal_icon_theme() -> Option<String> {
    static CACHED: OnceLock<Option<String>> = OnceLock::new();
    CACHED.get_or_init(fetch_portal_icon_theme).clone()
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
/// 2. Portal cross-desktop setting (`org.freedesktop.portal.Settings.Read`).
/// 3. System default symlink/file (`/usr/share/icons/default/index.theme` `Inherits`).
/// 4. Standard Freedesktop fallback (`"hicolor"`).
#[must_use]
pub fn resolve_active_theme(roots: &IconRoots) -> String {
    // 1. User config
    if let Some(user_theme) = read_configured_theme() {
        if theme_exists(&user_theme, roots) {
            return user_theme;
        }
    }

    // 2. Portal setting via xdg-desktop-portal
    if let Some(portal_theme) = read_portal_icon_theme() {
        if theme_exists(&portal_theme, roots) {
            return portal_theme;
        }
    }

    // 3. System default (/usr/share/icons/default/index.theme)
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

    // 4. Fallback to standard spec theme
    HICOLOR_THEME_NAME.to_string()
}
