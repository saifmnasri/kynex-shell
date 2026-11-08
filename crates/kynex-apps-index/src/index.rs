//! Applications index scanner and resolver across XDG data directories.
//!
//! Citations: Desktop Entry Specification (Version 1.5):
//! - Section "Desktop File ID"
//! - Section "File lookup"

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::entry::{DesktopEntry, DesktopEntryError, MAX_FILE_SIZE};
use crate::search::search_entries;

/// Applications index maintaining parsed, validated desktop entries with XDG precedence.
#[derive(Debug, Clone, Default)]
pub struct AppsIndex {
    entries: Vec<DesktopEntry>,
    by_id: HashMap<String, usize>,
    current_desktop: String,
}

impl AppsIndex {
    /// Creates a new empty `AppsIndex` for the specified desktop environment.
    #[must_use]
    pub fn new(current_desktop: impl Into<String>) -> Self {
        Self {
            entries: Vec::new(),
            by_id: HashMap::new(),
            current_desktop: current_desktop.into(),
        }
    }

    /// Returns standard XDG applications directory paths in precedence order.
    ///
    /// Desktop Entry Specification Section "File lookup":
    /// 1. `$XDG_DATA_HOME/applications` (default `$HOME/.local/share/applications`)
    /// 2. Each dir in `$XDG_DATA_DIRS/applications` (default `/usr/local/share/applications:/usr/share/applications`)
    #[must_use]
    pub fn standard_app_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            if !data_home.is_empty() {
                dirs.push(PathBuf::from(data_home).join("applications"));
            }
        } else if let Ok(home) = std::env::var("HOME") {
            dirs.push(
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("applications"),
            );
        }

        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
        for dir in std::env::split_paths(&data_dirs) {
            if !dir.as_os_str().is_empty() {
                dirs.push(dir.join("applications"));
            }
        }

        dirs
    }

    /// Loads and builds the index using the standard XDG environment directories.
    ///
    /// # Errors
    /// Returns `DesktopEntryError` if directory traversal fails catastrophically.
    pub fn load_standard(current_desktop: &str) -> Result<Self, DesktopEntryError> {
        let dirs = Self::standard_app_dirs();
        Self::load_from_dirs(&dirs, current_desktop)
    }

    /// Loads and indexes `.desktop` files from the specified directories in precedence order.
    ///
    /// Earlier directories override later ones for matching desktop file IDs.
    ///
    /// # Errors
    /// Returns `DesktopEntryError` on critical errors.
    pub fn load_from_dirs(
        dirs: &[PathBuf],
        current_desktop: &str,
    ) -> Result<Self, DesktopEntryError> {
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut entries = Vec::new();
        let mut by_id = HashMap::new();

        for base_dir in dirs {
            if !base_dir.is_dir() {
                continue;
            }

            let discovered = scan_dir_recursive(base_dir);
            for (id, file_path) in discovered {
                // If already seen from an earlier precedence directory, skip
                if seen_ids.contains(&id) {
                    continue;
                }
                seen_ids.insert(id.clone());

                match read_and_parse_entry(&id, &file_path, current_desktop) {
                    Ok(entry) => {
                        let idx = entries.len();
                        entries.push(entry);
                        by_id.insert(id, idx);
                    }
                    Err(
                        DesktopEntryError::Hidden
                        | DesktopEntryError::NoDisplay
                        | DesktopEntryError::DesktopNotAllowed(_),
                    ) => {
                        // Desktop Entry Specification Section "Recognized desktop entry keys":
                        // Hidden=true, NoDisplay=true, or desktop mismatch masks lower precedence.
                    }
                    Err(_other_err) => {
                        // Ignore malformed or unsupported entries
                    }
                }
            }
        }

        Ok(Self {
            entries,
            by_id,
            current_desktop: current_desktop.to_string(),
        })
    }

    /// Incrementally updates or removes a single entry for a modified/removed file.
    pub fn update_entry_file(&mut self, id: &str, file_path: &Path) {
        if !file_path.exists() {
            // Deleted: remove from index
            if let Some(idx) = self.by_id.remove(id) {
                if idx < self.entries.len() {
                    self.entries.remove(idx);
                    // Re-index map
                    self.by_id.clear();
                    for (i, entry) in self.entries.iter().enumerate() {
                        self.by_id.insert(entry.id.clone(), i);
                    }
                }
            }
            return;
        }

        match read_and_parse_entry(id, file_path, &self.current_desktop) {
            Ok(entry) => {
                if let Some(&idx) = self.by_id.get(id) {
                    self.entries[idx] = entry;
                } else {
                    let idx = self.entries.len();
                    self.entries.push(entry);
                    self.by_id.insert(id.to_string(), idx);
                }
            }
            Err(_) => {
                // If it became Hidden, NoDisplay, or invalid, remove if previously present
                if let Some(idx) = self.by_id.remove(id) {
                    if idx < self.entries.len() {
                        self.entries.remove(idx);
                        self.by_id.clear();
                        for (i, entry) in self.entries.iter().enumerate() {
                            self.by_id.insert(entry.id.clone(), i);
                        }
                    }
                }
            }
        }
    }

    /// Returns all loaded and valid application entries.
    #[must_use]
    pub fn entries(&self) -> &[DesktopEntry] {
        &self.entries
    }

    /// Returns the number of application entries in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Checks if the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the current desktop environment name configured for this index.
    #[must_use]
    pub fn current_desktop(&self) -> &str {
        &self.current_desktop
    }

    /// Looks up an application by its desktop file ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&DesktopEntry> {
        self.by_id.get(id).and_then(|&idx| self.entries.get(idx))
    }

    /// Performs ranked search over indexed applications.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize, locale: Option<&str>) -> Vec<&DesktopEntry> {
        search_entries(&self.entries, query, limit, locale)
    }
}

/// Recursively scans an applications base directory and yields `(id, file_path)`.
///
/// Desktop Entry Specification Section "Desktop File ID":
/// Slashes in subdirectories are mapped to hyphens.
fn scan_dir_recursive(base_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut results = Vec::new();
    let mut stack = vec![(base_dir.to_path_buf(), String::new())];
    let mut visited_dirs = HashSet::new();

    while let Some((dir, prefix)) = stack.pop() {
        let Ok(canonical_dir) = dir.canonicalize() else {
            continue;
        };
        if !visited_dirs.insert(canonical_dir) {
            // Avoid symlink loops
            continue;
        }

        let Ok(read_dir) = fs::read_dir(&dir) else {
            continue;
        };

        for entry_res in read_dir {
            let Ok(entry) = entry_res else {
                continue;
            };
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            if file_type.is_dir() {
                let new_prefix = if prefix.is_empty() {
                    name_str.to_string()
                } else {
                    format!("{prefix}-{name_str}")
                };
                stack.push((path, new_prefix));
            } else if (file_type.is_file() || file_type.is_symlink())
                && name_str.ends_with(".desktop")
            {
                let id = if prefix.is_empty() {
                    name_str.to_string()
                } else {
                    format!("{prefix}-{name_str}")
                };
                results.push((id, path));
            }
        }
    }

    results
}

/// Reads and parses a desktop entry from disk with security validations.
fn read_and_parse_entry(
    id: &str,
    path: &Path,
    current_desktop: &str,
) -> Result<DesktopEntry, DesktopEntryError> {
    let meta = fs::metadata(path).map_err(|e| DesktopEntryError::Io(e.to_string()))?;
    if !meta.is_file() {
        return Err(DesktopEntryError::NotARegularFile(
            path.display().to_string(),
        ));
    }

    if meta.len() > MAX_FILE_SIZE {
        return Err(DesktopEntryError::FileTooLarge(meta.len()));
    }

    let bytes = fs::read(path).map_err(|e| DesktopEntryError::Io(e.to_string()))?;
    DesktopEntry::parse_bytes(id, path, &bytes, current_desktop)
}
