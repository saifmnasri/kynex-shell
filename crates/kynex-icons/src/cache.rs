//! In-memory bounded cache and lazy directory indexing.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::theme::ThemeIndex;

/// Maximum number of cached icon lookup results.
pub const DEFAULT_CACHE_CAPACITY: usize = 1024;

/// Cache key for an icon lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LookupKey {
    /// Normalized icon name.
    pub icon_name: String,
    /// Requested pixel size.
    pub size: u32,
    /// Display scale.
    pub scale: u32,
    /// Active theme name.
    pub theme: String,
}

/// Thread-safe bounded LRU-style cache for icon lookups and lazy directory indices.
#[derive(Debug)]
pub struct IconCache {
    capacity: usize,
    lookup_results: Mutex<LookupStore>,
    theme_indices: Mutex<HashMap<String, Option<Arc<ThemeIndex>>>>,
    directory_files: Mutex<HashMap<PathBuf, Arc<HashSet<String>>>>,
}

#[derive(Debug)]
struct LookupStore {
    map: HashMap<LookupKey, Option<PathBuf>>,
    queue: VecDeque<LookupKey>,
}

impl Default for IconCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_CAPACITY)
    }
}

impl IconCache {
    /// Create a new cache with specified lookup capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(16),
            lookup_results: Mutex::new(LookupStore {
                map: HashMap::new(),
                queue: VecDeque::new(),
            }),
            theme_indices: Mutex::new(HashMap::new()),
            directory_files: Mutex::new(HashMap::new()),
        }
    }

    /// Retrieve a cached lookup result if present.
    pub fn get_lookup(&self, key: &LookupKey) -> Option<Option<PathBuf>> {
        let store = self.lookup_results.lock().ok()?;
        store.map.get(key).cloned()
    }

    /// Insert a lookup result into the bounded cache, evicting oldest entries on overflow.
    pub fn insert_lookup(&self, key: LookupKey, result: Option<PathBuf>) {
        if let Ok(mut store) = self.lookup_results.lock() {
            if !store.map.contains_key(&key) {
                if store.queue.len() >= self.capacity {
                    if let Some(oldest) = store.queue.pop_front() {
                        store.map.remove(&oldest);
                    }
                }
                store.queue.push_back(key.clone());
            }
            store.map.insert(key, result);
        }
    }

    /// Retrieve or lazily load a theme index for a given theme name across theme roots.
    pub fn get_or_load_theme_index<F>(&self, theme_name: &str, loader: F) -> Option<Arc<ThemeIndex>>
    where
        F: FnOnce(&str) -> Option<ThemeIndex>,
    {
        {
            if let Ok(indices) = self.theme_indices.lock() {
                if let Some(cached) = indices.get(theme_name) {
                    return cached.clone();
                }
            }
        }

        let new_entry = loader(theme_name).map(Arc::new);

        if let Ok(mut indices) = self.theme_indices.lock() {
            indices.insert(theme_name.to_string(), new_entry.clone());
        }

        new_entry
    }

    /// Retrieve or lazily index the list of regular filenames inside a directory.
    pub fn get_or_scan_directory(&self, dir_path: &Path) -> Arc<HashSet<String>> {
        {
            if let Ok(dirs) = self.directory_files.lock() {
                if let Some(cached) = dirs.get(dir_path) {
                    return cached.clone();
                }
            }
        }

        let mut files = HashSet::new();
        if let Ok(entries) = fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    files.insert(name);
                }
            }
        }

        let files_arc = Arc::new(files);

        if let Ok(mut dirs) = self.directory_files.lock() {
            dirs.insert(dir_path.to_path_buf(), files_arc.clone());
        }

        files_arc
    }

    /// Clear all cached lookup entries and directory indexes.
    pub fn clear(&self) {
        if let Ok(mut store) = self.lookup_results.lock() {
            store.map.clear();
            store.queue.clear();
        }
        if let Ok(mut indices) = self.theme_indices.lock() {
            indices.clear();
        }
        if let Ok(mut dirs) = self.directory_files.lock() {
            dirs.clear();
        }
    }
}
