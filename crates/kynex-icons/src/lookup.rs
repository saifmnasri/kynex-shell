//! Freedesktop Icon Theme Specification v0.13: Icon Lookup Algorithm.
//!
//! Citations:
//! - Section "Icon Lookup": `FindIcon`, `LookupIcon`, `LookupFallbackIcon`, `DirectoryMatchesSize`, `DirectorySizeDistance`.
//! - Section "Theme Description File": `Inherits`, parent theme chain, cycle detection.
//! - Section "Directory Layout": root search paths, unthemed `/usr/share/pixmaps`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::cache::{IconCache, LookupKey};
use crate::error::IconError;
use crate::roots::IconRoots;
use crate::theme::ThemeIndex;

/// Maximum allowed icon name length in characters.
pub const MAX_ICON_NAME_LENGTH: usize = 128;

/// Maximum inheritance depth for parent themes (preventing cyclic or pathologically deep inheritance).
pub const MAX_INHERITANCE_DEPTH: usize = 8;

/// Validate an icon name against allowed character sets and bounds.
///
/// Must:
/// - Be between 1 and 128 characters in length.
/// - Contain only ASCII alphanumeric characters, hyphens (`-`), underscores (`_`), or dots (`.`).
/// - Contain no path separators (`/`, `\`), path traversal (`..`), NUL bytes, or control characters.
///
/// # Errors
/// Returns `IconError::InvalidIconName` if any validation check fails.
pub fn validate_icon_name(name: &str) -> Result<&str, IconError> {
    if name.is_empty() {
        return Err(IconError::InvalidIconName {
            name: name.to_string(),
            reason: "icon name cannot be empty",
        });
    }

    if name.len() > MAX_ICON_NAME_LENGTH {
        return Err(IconError::InvalidIconName {
            name: name.to_string(),
            reason: "icon name exceeds maximum length (128 characters)",
        });
    }

    if name.contains('\0') || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(IconError::InvalidIconName {
            name: name.to_string(),
            reason: "icon name contains invalid path traversal or separator characters",
        });
    }

    for b in name.bytes() {
        if !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.') {
            return Err(IconError::InvalidIconName {
                name: name.to_string(),
                reason: "icon name contains forbidden characters outside [a-zA-Z0-9._-]",
            });
        }
    }

    Ok(name)
}

/// Normalize an icon name by stripping trailing `.png` or `.svg` extensions if present.
#[must_use]
pub fn normalize_icon_name(name: &str) -> &str {
    if let Some(stripped) = name.strip_suffix(".png") {
        stripped
    } else if let Some(stripped) = name.strip_suffix(".svg") {
        stripped
    } else {
        name
    }
}

/// Icon resolver engine encapsulating roots, caching, and theme resolution.
#[derive(Debug)]
pub struct IconResolver {
    roots: IconRoots,
    cache: Arc<IconCache>,
}

impl Default for IconResolver {
    fn default() -> Self {
        Self::new(IconRoots::from_env())
    }
}

impl IconResolver {
    /// Create a new resolver with the specified search roots.
    #[must_use]
    pub fn new(roots: IconRoots) -> Self {
        Self {
            roots,
            cache: Arc::new(IconCache::default()),
        }
    }

    /// Create a new resolver with custom roots and an explicit cache.
    #[must_use]
    pub fn with_cache(roots: IconRoots, cache: Arc<IconCache>) -> Self {
        Self { roots, cache }
    }

    /// Access the underlying search roots.
    #[must_use]
    pub fn roots(&self) -> &IconRoots {
        &self.roots
    }

    /// Access the internal cache.
    #[must_use]
    pub fn cache(&self) -> &IconCache {
        &self.cache
    }

    /// Freedesktop Icon Theme Specification Section "Icon Lookup": `FindIcon(icon, size, scale, theme)`.
    ///
    /// Resolves an icon name to an absolute file path adhering strictly to:
    /// 1. Absolute-path icon validation (if path begins with `/`).
    /// 2. Bounded cache lookup.
    /// 3. Theme search with exact-size match pass (`DirectoryMatchesSize`).
    /// 4. Theme search with closest-size distance pass (`DirectorySizeDistance`).
    /// 5. Parent theme chain recursion (`Inherits`) with cycle detection and depth cap (8).
    /// 6. Mandatory fallback to `hicolor` base theme.
    /// 7. Unthemed fallback in `/usr/share/pixmaps`.
    ///
    /// # Errors
    /// Returns `IconError` if the icon name is invalid or absolute path validation fails.
    pub fn find_icon(
        &self,
        icon_name: &str,
        size: u32,
        scale: u32,
        theme_name: &str,
    ) -> Result<Option<PathBuf>, IconError> {
        let trimmed = icon_name.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        // 1. Handle absolute paths from .desktop files
        if trimmed.starts_with('/') {
            let path = Path::new(trimmed);
            return match self.roots.validate_absolute_path_icon(path) {
                Ok(resolved) => Ok(Some(resolved)),
                Err(IconError::FileNotFound(_)) => Ok(None),
                Err(e) => Err(e),
            };
        }

        // Validate character set and length
        let validated = validate_icon_name(trimmed)?;
        let normalized = normalize_icon_name(validated);

        // 2. Check bounded lookup cache
        let cache_key = LookupKey {
            icon_name: normalized.to_string(),
            size,
            scale: scale.max(1),
            theme: theme_name.to_string(),
        };

        if let Some(cached_result) = self.cache.get_lookup(&cache_key) {
            return Ok(cached_result);
        }

        // 3. Resolve using theme hierarchy
        let resolved = self.lookup_in_theme_hierarchy(normalized, size, scale.max(1), theme_name);

        // Store result in bounded cache
        self.cache.insert_lookup(cache_key, resolved.clone());

        Ok(resolved)
    }

    /// Traverse the theme hierarchy (active theme -> parents -> hicolor -> pixmaps).
    fn lookup_in_theme_hierarchy(
        &self,
        icon_name: &str,
        size: u32,
        scale: u32,
        initial_theme: &str,
    ) -> Option<PathBuf> {
        let mut visited = HashSet::new();
        let mut queue = Vec::new();

        if !initial_theme.is_empty() {
            queue.push((initial_theme.to_string(), 0usize));
            visited.insert(initial_theme.to_string());
        }

        // Process theme queue (Breadth-First Search up to MAX_INHERITANCE_DEPTH)
        while let Some((theme, depth)) = queue.pop() {
            if let Some(path) = self.lookup_icon_in_theme(icon_name, size, scale, &theme) {
                return Some(path);
            }

            if depth < MAX_INHERITANCE_DEPTH {
                if let Some(theme_idx) = self.load_theme_index(&theme) {
                    for parent in &theme_idx.inherits {
                        let parent = parent.trim();
                        if !parent.is_empty() && visited.insert(parent.to_string()) {
                            queue.push((parent.to_string(), depth + 1));
                        }
                    }
                }
            }
        }

        // Mandatory fallback: hicolor (Section "Icon Lookup": parents = ["hicolor"])
        if visited.insert("hicolor".to_string()) {
            if let Some(path) = self.lookup_icon_in_theme(icon_name, size, scale, "hicolor") {
                return Some(path);
            }
        }

        // Unthemed fallback: /usr/share/pixmaps (Section "Icon Lookup": LookupFallbackIcon)
        self.lookup_fallback_pixmaps(icon_name)
    }

    /// Load or retrieve cached `ThemeIndex` for a theme name.
    fn load_theme_index(&self, theme_name: &str) -> Option<Arc<ThemeIndex>> {
        self.cache.get_or_load_theme_index(theme_name, |name| {
            for root in &self.roots.theme_roots {
                let theme_file = root.join(name).join("index.theme");
                if theme_file.is_file() {
                    if let Ok(parsed) = ThemeIndex::from_file(&theme_file) {
                        return Some(parsed);
                    }
                }
            }
            None
        })
    }

    /// Freedesktop Icon Theme Specification Section "Icon Lookup": `LookupIcon(icon, size, scale, theme)`.
    fn lookup_icon_in_theme(
        &self,
        icon_name: &str,
        size: u32,
        scale: u32,
        theme_name: &str,
    ) -> Option<PathBuf> {
        let theme_idx = self.load_theme_index(theme_name)?;

        // Pass 1: Exact matches (DirectoryMatchesSize)
        for subdir in &theme_idx.directories {
            if subdir.matches_size(size, scale) {
                if let Some(file) =
                    self.find_file_in_theme_subdir(theme_name, &subdir.name, icon_name)
                {
                    return Some(file);
                }
            }
        }

        // Pass 2: Closest matches (DirectorySizeDistance)
        let mut min_distance = u32::MAX;
        let mut closest_file = None;

        for subdir in &theme_idx.directories {
            let dist = subdir.size_distance(size, scale);
            if dist < min_distance {
                if let Some(file) =
                    self.find_file_in_theme_subdir(theme_name, &subdir.name, icon_name)
                {
                    min_distance = dist;
                    closest_file = Some(file);
                }
            }
        }

        closest_file
    }

    /// Search for `icon_name.png` or `icon_name.svg` in a theme subdirectory.
    fn find_file_in_theme_subdir(
        &self,
        theme_name: &str,
        subdir_name: &str,
        icon_name: &str,
    ) -> Option<PathBuf> {
        let png_name = format!("{icon_name}.png");
        let svg_name = format!("{icon_name}.svg");

        for root in &self.roots.theme_roots {
            let dir_path = root.join(theme_name).join(subdir_name);
            let files = self.cache.get_or_scan_directory(&dir_path);

            if files.contains(&png_name) {
                let candidate = dir_path.join(&png_name);
                if let Ok(resolved) = self.roots.validate_and_resolve_file(&candidate) {
                    return Some(resolved);
                }
            }

            if files.contains(&svg_name) {
                let candidate = dir_path.join(&svg_name);
                if let Ok(resolved) = self.roots.validate_and_resolve_file(&candidate) {
                    return Some(resolved);
                }
            }
        }

        None
    }

    /// Freedesktop Icon Theme Specification Section "Icon Lookup": `LookupFallbackIcon(icon)`.
    fn lookup_fallback_pixmaps(&self, icon_name: &str) -> Option<PathBuf> {
        let png_name = format!("{icon_name}.png");
        let svg_name = format!("{icon_name}.svg");

        for pixmap_root in &self.roots.pixmap_roots {
            let files = self.cache.get_or_scan_directory(pixmap_root);

            if files.contains(&png_name) {
                let candidate = pixmap_root.join(&png_name);
                if let Ok(resolved) = self.roots.validate_and_resolve_file(&candidate) {
                    return Some(resolved);
                }
            }

            if files.contains(&svg_name) {
                let candidate = pixmap_root.join(&svg_name);
                if let Ok(resolved) = self.roots.validate_and_resolve_file(&candidate) {
                    return Some(resolved);
                }
            }
        }

        None
    }
}
