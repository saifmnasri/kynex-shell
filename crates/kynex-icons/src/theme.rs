//! Freedesktop Icon Theme Specification v0.13: `index.theme` Parser and Directory Matching.
//!
//! Citations:
//! - Section "Theme Description File": format of `index.theme`, `[Icon Theme]` and per-directory sections.
//! - Section "Icon Lookup": `DirectoryMatchesSize` and `DirectorySizeDistance` formulas.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use crate::error::IconError;

/// Maximum allowed `index.theme` file size (512 KiB).
pub const MAX_THEME_FILE_SIZE: u64 = 512 * 1024;

/// Maximum line length allowed in `index.theme` (32 KiB, accommodating large `Directories=` lines).
pub const MAX_THEME_LINE_LENGTH: usize = 32 * 1024;

/// Maximum total lines parsed in an `index.theme` file.
pub const MAX_THEME_LINES: usize = 10_000;

/// Directory icon scaling type (Section "Theme Description File").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryType {
    /// Exact pixel size only.
    Fixed,
    /// Vector scalable within `MinSize` and `MaxSize`.
    Scalable,
    /// Bitmap icon valid within [Size - Threshold, Size + Threshold].
    Threshold,
}

impl Default for DirectoryType {
    /// Section "Theme Description File": "The default is Threshold."
    fn default() -> Self {
        Self::Threshold
    }
}

/// Metadata for an icon subdirectory (Section "Theme Description File").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubdirectoryMeta {
    /// Directory name relative to theme root (e.g. `48x48/apps` or `scalable/apps`).
    pub name: String,
    /// Nominal icon size in pixels (e.g. 16, 22, 32, 48, 64, 128, etc.).
    pub size: u32,
    /// Target scale factor (default 1).
    pub scale: u32,
    /// Directory type: Fixed, Scalable, or Threshold (default Threshold).
    pub dir_type: DirectoryType,
    /// Minimum size for Scalable (defaults to `size`).
    pub min_size: u32,
    /// Maximum size for Scalable (defaults to `size`).
    pub max_size: u32,
    /// Allowed deviation for Threshold (defaults to 2).
    pub threshold: u32,
}

impl SubdirectoryMeta {
    /// Freedesktop Icon Theme Specification Section "Icon Lookup": `DirectoryMatchesSize`.
    ///
    /// Determines whether this directory matches the requested icon size and display scale.
    #[must_use]
    pub fn matches_size(&self, icon_size: u32, icon_scale: u32) -> bool {
        if self.scale != icon_scale {
            return false;
        }

        match self.dir_type {
            DirectoryType::Fixed => self.size == icon_size,
            DirectoryType::Scalable => self.min_size <= icon_size && icon_size <= self.max_size,
            DirectoryType::Threshold => {
                let min = self.size.saturating_sub(self.threshold);
                let max = self.size.saturating_add(self.threshold);
                min <= icon_size && icon_size <= max
            }
        }
    }

    /// Freedesktop Icon Theme Specification Section "Icon Lookup": `DirectorySizeDistance`.
    ///
    /// Calculates the difference between the requested size and the directory's nominal size.
    #[must_use]
    pub fn size_distance(&self, icon_size: u32, icon_scale: u32) -> u32 {
        let target = icon_size.saturating_mul(icon_scale);

        match self.dir_type {
            DirectoryType::Fixed => {
                let actual = self.size.saturating_mul(self.scale);
                actual.abs_diff(target)
            }
            DirectoryType::Scalable => {
                let min = self.min_size.saturating_mul(self.scale);
                let max = self.max_size.saturating_mul(self.scale);
                if target < min {
                    min.saturating_sub(target)
                } else if target > max {
                    target.saturating_sub(max)
                } else {
                    0
                }
            }
            DirectoryType::Threshold => {
                let min = self
                    .size
                    .saturating_sub(self.threshold)
                    .saturating_mul(self.scale);
                let max = self
                    .size
                    .saturating_add(self.threshold)
                    .saturating_mul(self.scale);
                if target < min {
                    min.saturating_sub(target)
                } else if target > max {
                    target.saturating_sub(max)
                } else {
                    0
                }
            }
        }
    }
}

/// Parsed `index.theme` specification data for an icon theme.
#[derive(Debug, Clone)]
pub struct ThemeIndex {
    /// Human-readable theme name.
    pub name: String,
    /// Description/comment.
    pub comment: String,
    /// Comma-separated parent themes inherited by this theme (Section "Theme Description File").
    pub inherits: Vec<String>,
    /// List of subdirectory definitions configured for this theme.
    pub directories: Vec<SubdirectoryMeta>,
}

impl ThemeIndex {
    /// Parse an `index.theme` file from disk enforcing hard resource limits.
    ///
    /// # Errors
    /// Returns `IconError` if the file does not exist, exceeds 512 KiB, or violates formatting rules.
    pub fn from_file(path: &Path) -> Result<Self, IconError> {
        let meta = std::fs::symlink_metadata(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                IconError::FileNotFound(path.to_path_buf())
            } else {
                IconError::Io(e)
            }
        })?;

        if !meta.is_file() {
            return Err(IconError::NotARegularFile(path.to_path_buf()));
        }

        if meta.len() > MAX_THEME_FILE_SIZE {
            return Err(IconError::FileSizeExceeded {
                path: path.to_path_buf(),
                size: meta.len(),
                limit: MAX_THEME_FILE_SIZE,
            });
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file.take(MAX_THEME_FILE_SIZE + 1));
        Self::parse_reader(reader, path)
    }

    /// Parse `index.theme` content from any `BufRead` reader (used by tests and fuzzers).
    ///
    /// # Errors
    /// Returns `IconError` if line count or line length bounds are exceeded, or I/O fails.
    #[allow(clippy::too_many_lines)]
    pub fn parse_reader<R: BufRead>(mut reader: R, path: &Path) -> Result<Self, IconError> {
        let mut line_buf = String::new();
        let mut lines_count = 0usize;

        let mut current_section: Option<String> = None;
        let mut theme_name = String::new();
        let mut theme_comment = String::new();
        let mut inherits = Vec::new();
        let mut dir_names = Vec::new();

        // Subdirectory properties by section header
        let mut dir_props: HashMap<String, HashMap<String, String>> = HashMap::new();

        loop {
            line_buf.clear();
            let bytes_read = reader.read_line(&mut line_buf)?;
            if bytes_read == 0 {
                break;
            }

            lines_count += 1;
            if lines_count > MAX_THEME_LINES {
                return Err(IconError::InvalidThemeFile {
                    path: path.to_path_buf(),
                    reason: format!("exceeded maximum line count ({MAX_THEME_LINES})"),
                });
            }

            if line_buf.len() > MAX_THEME_LINE_LENGTH {
                return Err(IconError::InvalidThemeFile {
                    path: path.to_path_buf(),
                    reason: format!("line {lines_count} exceeded maximum length ({MAX_THEME_LINE_LENGTH} bytes)"),
                });
            }

            let line = line_buf.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Section header: [Section Name]
            if line.starts_with('[') && line.ends_with(']') {
                let section_name = line[1..line.len() - 1].trim().to_string();
                current_section = Some(section_name);
                continue;
            }

            // Key=Value pair
            if let Some((key, val)) = line.split_once('=') {
                let key = key.trim();
                let val = val.trim();

                match current_section.as_deref() {
                    Some("Icon Theme") => match key {
                        "Name" => theme_name = val.to_string(),
                        "Comment" => theme_comment = val.to_string(),
                        "Inherits" => {
                            inherits = val
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                        "Directories" | "ScaledDirectories" => {
                            for d in val.split(',') {
                                let d = d.trim();
                                if !d.is_empty() && !dir_names.contains(&d.to_string()) {
                                    dir_names.push(d.to_string());
                                }
                            }
                        }
                        _ => {}
                    },
                    Some(section) => {
                        dir_props
                            .entry(section.to_string())
                            .or_default()
                            .insert(key.to_string(), val.to_string());
                    }
                    None => {}
                }
            }
        }

        // Build list of valid subdirectory metadata
        let mut directories = Vec::new();
        for dir_name in dir_names {
            let Some(props) = dir_props.get(&dir_name) else {
                continue;
            };

            // Section "Theme Description File": Size is required
            let size = match props.get("Size").and_then(|s| s.parse::<u32>().ok()) {
                Some(s) if s > 0 && s <= 4096 => s,
                _ => continue,
            };

            let scale = props
                .get("Scale")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1)
                .max(1);

            let dir_type = match props.get("Type").map(std::string::String::as_str) {
                Some("Fixed") => DirectoryType::Fixed,
                Some("Scalable") => DirectoryType::Scalable,
                _ => DirectoryType::Threshold,
            };

            let min_size = props
                .get("MinSize")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(size);

            let max_size = props
                .get("MaxSize")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(size);

            let threshold = props
                .get("Threshold")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(2);

            directories.push(SubdirectoryMeta {
                name: dir_name,
                size,
                scale,
                dir_type,
                min_size,
                max_size,
                threshold,
            });
        }

        Ok(Self {
            name: theme_name,
            comment: theme_comment,
            inherits,
            directories,
        })
    }
}
