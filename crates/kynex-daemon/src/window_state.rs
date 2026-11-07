//! Pure-Rust window state management and input sanitization.
//!
//! Enforces security bounds on untrusted window metadata supplied by window managers:
//! - Window ID: <= 64 ASCII chars of `[0-9A-Fa-f{}-]`.
//! - Window Title: <= 256 bytes UTF-8, cut on char boundaries; control chars and
//!   bidi/override chars (U+202A-202E, U+2066-2069) stripped.
//! - App ID: <= 128 chars of `[A-Za-z0-9._-]`, else empty.
//! - Capacity: at most 512 tracked windows.

use kynex_ipc::WindowItem;

/// Maximum number of simultaneously tracked windows.
pub const MAX_WINDOWS: usize = 512;

/// Maximum allowed length in bytes for window IDs.
pub const MAX_ID_LEN: usize = 64;

/// Maximum allowed length in bytes for window titles.
pub const MAX_TITLE_BYTES: usize = 256;

/// Maximum allowed length in bytes for app IDs.
pub const MAX_APP_ID_LEN: usize = 128;

/// Result of an upsert operation into the window state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpsertResult {
    /// A new window was successfully tracked.
    Added(WindowItem),
    /// An existing window was updated with modified fields.
    Changed(WindowItem),
    /// An existing window was updated but no field values changed.
    Unchanged,
    /// Cannot add new window because maximum capacity (512) was reached.
    CapacityExceeded,
    /// Input failed security validation.
    InvalidInput(String),
}

/// Sanitizes a window ID string.
/// Must be non-empty, <= 64 characters, containing only `[0-9A-Fa-f{}-]`.
#[must_use]
pub fn sanitize_id(raw_id: &str) -> Option<String> {
    if raw_id.is_empty() || raw_id.len() > MAX_ID_LEN {
        return None;
    }
    if raw_id
        .chars()
        .all(|c| c.is_ascii_hexdigit() || c == '{' || c == '}' || c == '-')
    {
        Some(raw_id.to_string())
    } else {
        None
    }
}

/// Sanitizes a window title string.
/// - Drops ASCII control characters (0x00..=0x1F, 0x7F).
/// - Drops Unicode directional/bidi override characters (U+202A..=U+202E, U+2066..=U+2069).
/// - Truncates to at most 256 bytes on a valid UTF-8 character boundary.
#[must_use]
pub fn sanitize_title(raw_title: &str) -> String {
    let mut filtered = String::with_capacity(raw_title.len().min(MAX_TITLE_BYTES));
    for c in raw_title.chars() {
        if c.is_control() {
            continue;
        }
        // Check for Unicode bidi/override characters
        // U+202A (LRE), U+202B (RLE), U+202C (PDF), U+202D (LRO), U+202E (RLO)
        // U+2066 (LRI), U+2067 (RLI), U+2068 (FSI), U+2069 (PDI)
        if matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
            continue;
        }
        filtered.push(c);
    }

    if filtered.len() <= MAX_TITLE_BYTES {
        return filtered;
    }

    // Cut on UTF-8 char boundary <= MAX_TITLE_BYTES
    let mut cut_index = MAX_TITLE_BYTES;
    while cut_index > 0 && !filtered.is_char_boundary(cut_index) {
        cut_index -= 1;
    }
    filtered.truncate(cut_index);
    filtered
}

/// Sanitizes an application desktop ID / class name.
/// Must be <= 128 characters of `[A-Za-z0-9._-]`. If invalid or overly long, returns `""`.
#[must_use]
pub fn sanitize_app_id(raw_app_id: &str) -> String {
    if raw_app_id.len() > MAX_APP_ID_LEN {
        return String::new();
    }
    if raw_app_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        raw_app_id.to_string()
    } else {
        String::new()
    }
}

/// Pure state machine tracking active windows in memory.
#[derive(Debug, Default, Clone)]
pub struct WindowState {
    windows: Vec<WindowItem>,
}

impl WindowState {
    /// Creates a new empty `WindowState`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
        }
    }

    /// Resets all tracked windows, returning whether any windows were present.
    pub fn reset(&mut self) -> bool {
        if self.windows.is_empty() {
            false
        } else {
            self.windows.clear();
            true
        }
    }

    /// Returns a slice of all tracked windows in deterministic order.
    #[must_use]
    pub fn list(&self) -> &[WindowItem] {
        &self.windows
    }

    /// Returns the number of currently tracked windows.
    #[must_use]
    pub fn count(&self) -> usize {
        self.windows.len()
    }

    /// Returns the currently active window ID, or `None` if no window has active focus.
    #[must_use]
    pub fn active_id(&self) -> Option<&str> {
        self.windows
            .iter()
            .find(|w| w.active)
            .map(|w| w.id.as_str())
    }

    /// Inserts or updates window metadata.
    pub fn upsert(
        &mut self,
        raw_id: &str,
        raw_title: &str,
        raw_app_id: &str,
        minimized: bool,
        active: bool,
    ) -> UpsertResult {
        let Some(id) = sanitize_id(raw_id) else {
            return UpsertResult::InvalidInput(format!("Invalid window ID: '{raw_id}'"));
        };
        let title = sanitize_title(raw_title);
        let app_id = sanitize_app_id(raw_app_id);

        // If this window is becoming active, deactivate all other windows
        if active {
            for win in &mut self.windows {
                if win.id != id {
                    win.active = false;
                }
            }
        }

        if let Some(existing) = self.windows.iter_mut().find(|w| w.id == id) {
            let changed = existing.title != title
                || existing.app_id != app_id
                || existing.minimized != minimized
                || existing.active != active;

            if changed {
                existing.title = title;
                existing.app_id = app_id;
                existing.minimized = minimized;
                existing.active = active;
                UpsertResult::Changed(existing.clone())
            } else {
                UpsertResult::Unchanged
            }
        } else {
            if self.windows.len() >= MAX_WINDOWS {
                return UpsertResult::CapacityExceeded;
            }
            let item = WindowItem {
                id,
                title,
                app_id,
                minimized,
                active,
            };
            self.windows.push(item.clone());
            UpsertResult::Added(item)
        }
    }

    /// Removes a window by ID. Returns the removed `WindowItem` if found.
    pub fn remove(&mut self, raw_id: &str) -> Option<WindowItem> {
        let id = sanitize_id(raw_id)?;
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            Some(self.windows.remove(pos))
        } else {
            None
        }
    }

    /// Updates the active window focus.
    /// If `raw_id` is empty or not found, clears active focus from all windows.
    /// Returns `true` if the active window actually changed.
    pub fn set_active(&mut self, raw_id: &str) -> bool {
        let prev_active = self.active_id().map(ToString::to_string);
        let target_id = sanitize_id(raw_id);

        for win in &mut self.windows {
            win.active = target_id.as_deref() == Some(&win.id);
        }

        let new_active = self.active_id().map(ToString::to_string);
        prev_active != new_active
    }

    /// Returns `true` if a window matching `raw_id` exists in currently tracked state.
    #[must_use]
    pub fn has_window(&self, raw_id: &str) -> bool {
        let Some(id) = sanitize_id(raw_id) else {
            return false;
        };
        self.windows.iter().any(|w| w.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_id_valid() {
        assert_eq!(sanitize_id("123"), Some("123".to_string()));
        assert_eq!(
            sanitize_id("{c7c4f456-4c47-49c8-89c0-6d43e5900593}"),
            Some("{c7c4f456-4c47-49c8-89c0-6d43e5900593}".to_string())
        );
        assert_eq!(
            sanitize_id("dead-beef-cafe"),
            Some("dead-beef-cafe".to_string())
        );
    }

    #[test]
    fn test_sanitize_id_invalid() {
        assert_eq!(sanitize_id(""), None);
        assert_eq!(sanitize_id("win 123"), None); // space
        assert_eq!(sanitize_id("win$123"), None); // invalid char
        assert_eq!(sanitize_id("win/123"), None); // slash
        let long_id = "a".repeat(65);
        assert_eq!(sanitize_id(&long_id), None);
    }

    #[test]
    fn test_sanitize_title_clean() {
        assert_eq!(sanitize_title("Terminal"), "Terminal");
        assert_eq!(sanitize_title("Hello World! 123"), "Hello World! 123");
    }

    #[test]
    fn test_sanitize_title_control_chars() {
        let dirty = "Term\x00in\x1Bal\r\n\t";
        assert_eq!(sanitize_title(dirty), "Terminal");
    }

    #[test]
    fn test_sanitize_title_bidi_overrides() {
        // U+202E (RLO) and U+2066 (LRI)
        let hostile = "Normal\u{202E}Spoofed\u{2066}Title";
        assert_eq!(sanitize_title(hostile), "NormalSpoofedTitle");
    }

    #[test]
    fn test_sanitize_title_truncation_utf8() {
        let long = "a".repeat(300);
        let sanitized = sanitize_title(&long);
        assert_eq!(sanitized.len(), MAX_TITLE_BYTES);

        // Truncate cleanly across multi-byte boundary:
        // '🦀' is 4 bytes. 63 * 4 = 252 bytes. 64 * 4 = 256 bytes.
        let crab = "🦀".repeat(70);
        let crab_sanitized = sanitize_title(&crab);
        assert_eq!(crab_sanitized.len(), 256);
        assert!(std::str::from_utf8(crab_sanitized.as_bytes()).is_ok());

        // Multi-byte crossing 256 byte boundary:
        // 254 'a's + one '🦀' (4 bytes) = 258 bytes.
        // Must cut at 254 bytes without splitting the crab!
        let mut mixed = "a".repeat(254);
        mixed.push('🦀');
        let mixed_sanitized = sanitize_title(&mixed);
        assert_eq!(mixed_sanitized.len(), 254);
        assert_eq!(mixed_sanitized, "a".repeat(254));
    }

    #[test]
    fn test_sanitize_app_id() {
        assert_eq!(sanitize_app_id("org.kde.konsole"), "org.kde.konsole");
        assert_eq!(sanitize_app_id("firefox_esr-1"), "firefox_esr-1");
        assert_eq!(sanitize_app_id("invalid app id!"), "");
        assert_eq!(sanitize_app_id("bad/path"), "");
        let long_app = "a".repeat(129);
        assert_eq!(sanitize_app_id(&long_app), "");
    }

    #[test]
    fn test_upsert_lifecycle_and_capacity() {
        let mut state = WindowState::new();
        assert_eq!(state.count(), 0);

        // Add first window
        let res1 = state.upsert("{1}", "Window 1", "app.one", false, true);
        assert_eq!(
            res1,
            UpsertResult::Added(WindowItem {
                id: "{1}".into(),
                title: "Window 1".into(),
                app_id: "app.one".into(),
                minimized: false,
                active: true,
            })
        );
        assert_eq!(state.count(), 1);
        assert_eq!(state.active_id(), Some("{1}"));

        // Add second window as active -> first window must be deactivated
        let res2 = state.upsert("{2}", "Window 2", "app.two", false, true);
        assert!(matches!(res2, UpsertResult::Added(_)));
        assert_eq!(state.count(), 2);
        assert_eq!(state.active_id(), Some("{2}"));
        assert!(!state.list()[0].active);
        assert!(state.list()[1].active);

        // Update second window
        let res3 = state.upsert("{2}", "Window 2 Modified", "app.two", true, true);
        assert_eq!(
            res3,
            UpsertResult::Changed(WindowItem {
                id: "{2}".into(),
                title: "Window 2 Modified".into(),
                app_id: "app.two".into(),
                minimized: true,
                active: true,
            })
        );

        // Unchanged upsert
        let res4 = state.upsert("{2}", "Window 2 Modified", "app.two", true, true);
        assert_eq!(res4, UpsertResult::Unchanged);

        // Capacity bound test
        state.reset();
        for i in 0..MAX_WINDOWS {
            let id = format!("cafe-{i:04x}");
            let res = state.upsert(&id, "Title", "app", false, false);
            assert!(matches!(res, UpsertResult::Added(_)));
        }
        assert_eq!(state.count(), MAX_WINDOWS);

        // Non-hex ID is rejected
        let invalid_id_res = state.upsert("beef-overflow", "Title", "app", false, false);
        assert!(matches!(invalid_id_res, UpsertResult::InvalidInput(_)));
        // Note: "overflow" has 'o','r','v','l','w' which are non-hex, so let's test with valid hex
        let overflow_valid_hex = format!("dead-{:04x}", 9999);
        let res_overflow = state.upsert(&overflow_valid_hex, "Title", "app", false, false);
        assert_eq!(res_overflow, UpsertResult::CapacityExceeded);
        assert_eq!(state.count(), MAX_WINDOWS);

        // Existing window update among 512 is still permitted
        let update_existing = state.upsert("cafe-0000", "New Title", "app", true, false);
        assert!(matches!(update_existing, UpsertResult::Changed(_)));

        // Remove one window and add another
        assert!(state.remove("cafe-0000").is_some());
        assert_eq!(state.count(), MAX_WINDOWS - 1);
        let add_after_remove = state.upsert("cafe-9999", "Title", "app", false, false);
        assert!(matches!(add_after_remove, UpsertResult::Added(_)));
        assert_eq!(state.count(), MAX_WINDOWS);
    }

    #[test]
    fn test_set_active_and_remove() {
        let mut state = WindowState::new();
        state.upsert("a", "Title A", "app", false, false);
        state.upsert("b", "Title B", "app", false, false);
        assert_eq!(state.active_id(), None);

        // Set active window 'b'
        assert!(state.set_active("b"));
        assert_eq!(state.active_id(), Some("b"));

        // Setting same active returns false
        assert!(!state.set_active("b"));

        // Clear active
        assert!(state.set_active(""));
        assert_eq!(state.active_id(), None);

        // Remove non-existent returns None
        assert!(state.remove("non-existent").is_none());

        // Remove existent
        let removed = state.remove("a");
        assert_eq!(removed.map(|w| w.id), Some("a".into()));
        assert_eq!(state.count(), 1);
    }

    #[test]
    fn test_has_window() {
        let mut state = WindowState::new();
        state.upsert("cafe-1", "App", "app", false, false);
        assert!(state.has_window("cafe-1"));
        assert!(!state.has_window("cafe-2"));
        assert!(!state.has_window("win$invalid"));
        assert!(!state.has_window(""));
    }
}
