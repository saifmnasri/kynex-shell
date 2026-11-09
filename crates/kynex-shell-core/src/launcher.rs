//! Pure-Rust logic for the launcher overlay: selection, sequence-based latest-wins,
//! untrusted display text sanitization, and state management.

use kynex_ipc::apps::AppItem;

/// Maximum character length for application display name.
pub const MAX_NAME_LEN: usize = 128;
/// Maximum character length for generic name.
pub const MAX_GENERIC_NAME_LEN: usize = 128;
/// Maximum character length for comment description.
pub const MAX_COMMENT_LEN: usize = 256;
/// Maximum character length for search query.
pub const MAX_QUERY_LEN: usize = 128;
/// Maximum results returned by launcher.
pub const MAX_LAUNCHER_RESULTS: u32 = 50;

/// Sanitized application item prepared for safe UI rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDisplayItem {
    /// Desktop entry ID (e.g. `org.kde.konsole.desktop`).
    pub id: String,
    /// Sanitized display name.
    pub name: String,
    /// Sanitized generic name (e.g. `Terminal Emulator`).
    pub generic_name: String,
    /// Sanitized comment description.
    pub comment: String,
    /// Single-character placeholder badge for icon-free rendering.
    pub letter_badge: String,
}

/// Sanitizes untrusted display strings from external `.desktop` files.
///
/// Strips ASCII control characters (`0x00..=0x1F`, `0x7F`), Unicode directional/bidi override
/// characters (`U+202A..=U+202E`, `U+2066..=U+2069`), and truncates to `max_len` UTF-8 characters.
#[must_use]
pub fn sanitize_display_text(raw: &str, max_len: usize) -> String {
    let mut clean = String::with_capacity(raw.len().min(max_len));
    for c in raw.chars() {
        if clean.chars().count() >= max_len {
            break;
        }
        // Filter out ASCII/Unicode control characters
        if c.is_control() {
            continue;
        }
        // Filter out Unicode bidirectional formatting / override characters
        // U+202A (LRE), U+202B (RLE), U+202C (PDF), U+202D (LRO), U+202E (RLO)
        // U+2066 (LRI), U+2067 (RLI), U+2068 (FSI), U+2069 (PDI)
        match c {
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => {}
            _ => clean.push(c),
        }
    }
    clean.trim().to_string()
}

/// Sanitizes search query input from the user.
#[must_use]
pub fn sanitize_query(raw: &str) -> String {
    sanitize_display_text(raw, MAX_QUERY_LEN)
}

/// Computes letter placeholder badge from sanitized application name or ID.
#[must_use]
pub fn compute_letter_badge(name: &str, id: &str) -> String {
    for c in name.chars() {
        if c.is_alphanumeric() {
            return c.to_uppercase().to_string();
        }
    }
    for c in id.chars() {
        if c.is_alphanumeric() {
            return c.to_uppercase().to_string();
        }
    }
    "?".to_string()
}

/// Converts raw IPC `AppItem` into sanitized `AppDisplayItem`.
#[must_use]
pub fn sanitize_app_item(item: &AppItem) -> AppDisplayItem {
    let mut name = sanitize_display_text(&item.name, MAX_NAME_LEN);
    let generic_name = sanitize_display_text(&item.generic_name, MAX_GENERIC_NAME_LEN);
    let comment = sanitize_display_text(&item.comment, MAX_COMMENT_LEN);

    // Fallback if name is empty after sanitization
    if name.is_empty() {
        let fallback = item.id.strip_suffix(".desktop").unwrap_or(&item.id);
        name = sanitize_display_text(fallback, MAX_NAME_LEN);
        if name.is_empty() {
            name = "Unknown Application".to_string();
        }
    }

    let letter_badge = compute_letter_badge(&name, &item.id);

    AppDisplayItem {
        id: item.id.clone(),
        name,
        generic_name,
        comment,
        letter_badge,
    }
}

/// Selection management with circular wrap-around navigation.
pub struct SelectionState;

impl SelectionState {
    /// Computes next selected index with wrap-around to top (0).
    #[must_use]
    pub fn select_next(current: i32, total: usize) -> i32 {
        let count = i32::try_from(total).unwrap_or(0);
        if count <= 0 {
            return -1;
        }
        if current < 0 || current + 1 >= count {
            0
        } else {
            current + 1
        }
    }

    /// Computes previous selected index with wrap-around to bottom (`total - 1`).
    #[must_use]
    pub fn select_prev(current: i32, total: usize) -> i32 {
        let count = i32::try_from(total).unwrap_or(0);
        if count <= 0 {
            return -1;
        }
        if current <= 0 || current >= count {
            count - 1
        } else {
            current - 1
        }
    }
}

/// Sequence-based latest-wins tracker for asynchronous search queries.
///
/// Prevents slow stale query replies from overwriting newer query results.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LatestWinsTracker {
    current_seq: u64,
    last_applied_seq: u64,
}

impl LatestWinsTracker {
    /// Creates a new `LatestWinsTracker`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Generates next query sequence number.
    pub fn next_seq(&mut self) -> u64 {
        self.current_seq = self.current_seq.wrapping_add(1);
        self.current_seq
    }

    /// Returns `true` if reply with `seq` should be applied (i.e. `seq >= last_applied_seq`).
    pub fn should_apply(&mut self, seq: u64) -> bool {
        if seq >= self.last_applied_seq {
            self.last_applied_seq = seq;
            true
        } else {
            false
        }
    }

    /// Returns the currently active sequence number.
    #[must_use]
    pub fn current_seq(&self) -> u64 {
        self.current_seq
    }

    /// Returns the last applied sequence number.
    #[must_use]
    pub fn last_applied_seq(&self) -> u64 {
        self.last_applied_seq
    }
}

/// Pure-Rust model state and coordinator for the Launcher.
#[derive(Debug, Clone)]
pub struct LauncherCore {
    items: Vec<AppDisplayItem>,
    selected_index: i32,
    query: String,
    tracker: LatestWinsTracker,
}

impl Default for LauncherCore {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected_index: -1,
            query: String::new(),
            tracker: LatestWinsTracker::default(),
        }
    }
}

impl LauncherCore {
    /// Creates a new, empty `LauncherCore`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the currently indexed application items.
    #[must_use]
    pub fn items(&self) -> &[AppDisplayItem] {
        &self.items
    }

    /// Returns the number of application items.
    #[must_use]
    pub fn count(&self) -> usize {
        self.items.len()
    }

    /// Returns the currently selected row index (-1 if none/empty).
    #[must_use]
    pub fn selected_index(&self) -> i32 {
        self.selected_index
    }

    /// Sets the selected row index with bounds checking.
    pub fn set_selected_index(&mut self, idx: i32) {
        if self.items.is_empty() {
            self.selected_index = -1;
            return;
        }
        if let Ok(u) = usize::try_from(idx) {
            if u < self.items.len() {
                self.selected_index = idx;
                return;
            }
        }
        self.selected_index = -1;
    }

    /// Returns the active search query.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Returns the currently active sequence number.
    #[must_use]
    pub fn current_seq(&self) -> u64 {
        self.tracker.current_seq()
    }

    /// Prepares a new search query: sanitizes text, updates internal query,
    /// increments sequence tracker, and returns `(seq, clean_query)`.
    pub fn prepare_search_query(&mut self, raw: &str) -> (u64, String) {
        let clean = sanitize_query(raw);
        self.query.clone_from(&clean);
        let seq = self.tracker.next_seq();
        (seq, clean)
    }

    /// Applies new raw `AppItem` search results if `seq` passes latest-wins check.
    ///
    /// Returns `true` if results were applied, or `false` if dropped as stale.
    pub fn apply_results(&mut self, seq: u64, raw_items: &[AppItem]) -> bool {
        if !self.tracker.should_apply(seq) {
            return false;
        }

        self.items = raw_items.iter().map(sanitize_app_item).collect();
        if self.items.is_empty() {
            self.selected_index = -1;
        } else {
            self.selected_index = 0;
        }
        true
    }

    /// Moves selection down with wrap-around, returning the new selected index.
    pub fn select_next(&mut self) -> i32 {
        let next = SelectionState::select_next(self.selected_index, self.items.len());
        self.selected_index = next;
        next
    }

    /// Moves selection up with wrap-around, returning the new selected index.
    pub fn select_prev(&mut self) -> i32 {
        let prev = SelectionState::select_prev(self.selected_index, self.items.len());
        self.selected_index = prev;
        prev
    }

    /// Returns the desktop application ID to launch for row `index`, or `None` if out of bounds.
    #[must_use]
    pub fn launch(&self, index: i32) -> Option<String> {
        let idx = usize::try_from(index).ok()?;
        let item = self.items.get(idx)?;
        Some(item.id.clone())
    }

    /// Returns the desktop application ID for the currently selected item, or `None` if none selected.
    #[must_use]
    pub fn launch_selected(&self) -> Option<String> {
        self.launch(self.selected_index)
    }

    /// Resets launcher state on close: clears query, resets sequence for empty query,
    /// and returns `(seq, empty_query)`.
    pub fn close(&mut self) -> (u64, String) {
        self.query.clear();
        let seq = self.tracker.next_seq();
        (seq, String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app_item(id: &str, name: &str, generic_name: &str, comment: &str) -> AppItem {
        AppItem {
            id: id.to_string(),
            name: name.to_string(),
            generic_name: generic_name.to_string(),
            comment: comment.to_string(),
            icon_name: String::new(),
            categories: Vec::new(),
        }
    }

    #[test]
    fn test_selection_wrap_around() {
        assert_eq!(SelectionState::select_next(0, 0), -1);
        assert_eq!(SelectionState::select_prev(0, 0), -1);

        // 1 item
        assert_eq!(SelectionState::select_next(0, 1), 0);
        assert_eq!(SelectionState::select_prev(0, 1), 0);
        assert_eq!(SelectionState::select_next(-1, 1), 0);
        assert_eq!(SelectionState::select_prev(-1, 1), 0);

        // 3 items (indices 0, 1, 2)
        assert_eq!(SelectionState::select_next(0, 3), 1);
        assert_eq!(SelectionState::select_next(1, 3), 2);
        assert_eq!(SelectionState::select_next(2, 3), 0); // wrap to top

        assert_eq!(SelectionState::select_prev(2, 3), 1);
        assert_eq!(SelectionState::select_prev(1, 3), 0);
        assert_eq!(SelectionState::select_prev(0, 3), 2); // wrap to bottom
    }

    #[test]
    fn test_latest_wins_sequence_ordering() {
        let mut tracker = LatestWinsTracker::new();
        assert_eq!(tracker.current_seq(), 0);
        assert_eq!(tracker.last_applied_seq(), 0);

        let seq1 = tracker.next_seq();
        let seq2 = tracker.next_seq();
        let seq3 = tracker.next_seq();

        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
        assert_eq!(seq3, 3);

        // Applying seq 2 first (arrived out of order)
        assert!(tracker.should_apply(seq2));
        assert_eq!(tracker.last_applied_seq(), 2);

        // Stale seq 1 arrives later -> rejected
        assert!(!tracker.should_apply(seq1));
        assert_eq!(tracker.last_applied_seq(), 2);

        // Newer seq 3 arrives -> accepted
        assert!(tracker.should_apply(seq3));
        assert_eq!(tracker.last_applied_seq(), 3);

        // Duplicate seq 3 arrives -> accepted (idempotent)
        assert!(tracker.should_apply(seq3));
    }

    #[test]
    fn test_sanitize_display_text_bidi_override_characters() {
        let hostile = "Text\u{202E}gnirtS\u{202C}More\u{2066}Isolate\u{2069}";
        let clean = sanitize_display_text(hostile, 100);
        assert_eq!(clean, "TextgnirtSMoreIsolate");
    }

    #[test]
    fn test_sanitize_display_text_control_characters() {
        let hostile = "Line\x00Break\x07Alert\x1b[31mRed\x7fDelete";
        let clean = sanitize_display_text(hostile, 100);
        assert_eq!(clean, "LineBreakAlert[31mRedDelete");
    }

    #[test]
    fn test_sanitize_display_text_length_capping() {
        let long_str = "a".repeat(300);
        let clean = sanitize_display_text(&long_str, 50);
        assert_eq!(clean.chars().count(), 50);
    }

    #[test]
    fn test_sanitize_query() {
        let raw = "  terminal\x00\u{202A}  ";
        let clean = sanitize_query(raw);
        assert_eq!(clean, "terminal");
    }

    #[test]
    fn test_compute_letter_badge() {
        assert_eq!(compute_letter_badge("Firefox", "firefox.desktop"), "F");
        assert_eq!(
            compute_letter_badge("  1Password", "1password.desktop"),
            "1"
        );
        assert_eq!(compute_letter_badge("🚀 Rocket", "rocket.desktop"), "R");
        assert_eq!(compute_letter_badge("---", "app.desktop"), "A");
        assert_eq!(compute_letter_badge("", ""), "?");
    }

    #[test]
    fn test_sanitize_app_item_fallback() {
        let item = make_app_item(
            "org.kde.konsole.desktop",
            "\x00\x07",
            "Terminal Emulator\u{202E}",
            "Command line\x1b",
        );

        let sanitized = sanitize_app_item(&item);
        assert_eq!(sanitized.id, "org.kde.konsole.desktop");
        assert_eq!(sanitized.name, "org.kde.konsole");
        assert_eq!(sanitized.generic_name, "Terminal Emulator");
        assert_eq!(sanitized.comment, "Command line");
        assert_eq!(sanitized.letter_badge, "O");
    }

    #[test]
    fn test_sanitize_app_item_unknown_fallback() {
        let item = make_app_item(".desktop", "", "", "");
        let sanitized = sanitize_app_item(&item);
        assert_eq!(sanitized.name, "Unknown Application");
        assert_eq!(sanitized.letter_badge, "U");
    }

    #[test]
    fn test_launcher_core_launch_bounds_and_off_by_one_protection() {
        let mut core = LauncherCore::new();
        assert_eq!(core.count(), 0);
        assert_eq!(core.selected_index(), -1);
        assert_eq!(core.launch_selected(), None);
        assert_eq!(core.launch(0), None);
        assert_eq!(core.launch(-1), None);
        assert_eq!(core.launch(100), None);

        let items = vec![
            make_app_item("app0.desktop", "App 0", "", ""),
            make_app_item("app1.desktop", "App 1", "", ""),
            make_app_item("app2.desktop", "App 2", "", ""),
        ];

        let applied = core.apply_results(1, &items);
        assert!(applied);
        assert_eq!(core.count(), 3);
        assert_eq!(core.selected_index(), 0);

        // Precise index verification (kills M08 off-by-one mutant)
        assert_eq!(core.launch(0), Some("app0.desktop".to_string()));
        assert_eq!(core.launch(1), Some("app1.desktop".to_string()));
        assert_eq!(core.launch(2), Some("app2.desktop".to_string()));
        assert_eq!(core.launch(3), None); // Out of bounds upper
        assert_eq!(core.launch(-1), None); // Out of bounds lower

        // Launch selected index
        assert_eq!(core.launch_selected(), Some("app0.desktop".to_string()));
        core.select_next();
        assert_eq!(core.selected_index(), 1);
        assert_eq!(core.launch_selected(), Some("app1.desktop".to_string()));
        core.select_next();
        assert_eq!(core.selected_index(), 2);
        assert_eq!(core.launch_selected(), Some("app2.desktop".to_string()));
        core.select_next(); // Wrap
        assert_eq!(core.selected_index(), 0);
        assert_eq!(core.launch_selected(), Some("app0.desktop".to_string()));

        // Wrap prev
        core.select_prev();
        assert_eq!(core.selected_index(), 2);
        assert_eq!(core.launch_selected(), Some("app2.desktop".to_string()));

        // Explicit set_selected_index
        core.set_selected_index(1);
        assert_eq!(core.selected_index(), 1);
        core.set_selected_index(10); // Out of bounds
        assert_eq!(core.selected_index(), -1);
        assert_eq!(core.launch_selected(), None);
    }

    #[test]
    fn test_launcher_core_query_and_close() {
        let mut core = LauncherCore::new();
        let (seq1, q1) = core.prepare_search_query("  terminal  ");
        assert_eq!(seq1, 1);
        assert_eq!(q1, "terminal");
        assert_eq!(core.query(), "terminal");

        let (seq2, empty) = core.close();
        assert_eq!(seq2, 2);
        assert_eq!(empty, "");
        assert_eq!(core.query(), "");
    }

    #[test]
    fn test_launcher_core_stale_results_dropped() {
        let mut core = LauncherCore::new();
        let (seq1, _) = core.prepare_search_query("first");
        let (seq2, _) = core.prepare_search_query("second");

        let items2 = vec![make_app_item("app2.desktop", "Second App", "", "")];
        assert!(core.apply_results(seq2, &items2));
        assert_eq!(core.count(), 1);
        assert_eq!(core.items()[0].name, "Second App");

        // Stale reply from seq1 arriving late
        let items1 = vec![make_app_item("app1.desktop", "First App", "", "")];
        assert!(!core.apply_results(seq1, &items1));
        // State remains strictly from seq2
        assert_eq!(core.count(), 1);
        assert_eq!(core.items()[0].name, "Second App");
    }

    #[test]
    fn test_launcher_core_empty_results_resets_selection() {
        let mut core = LauncherCore::new();
        let items = vec![make_app_item("app.desktop", "App", "", "")];
        assert!(core.apply_results(1, &items));
        assert_eq!(core.selected_index(), 0);

        // Apply empty results
        assert!(core.apply_results(2, &[]));
        assert_eq!(core.count(), 0);
        assert_eq!(core.selected_index(), -1);
        assert_eq!(core.launch_selected(), None);
    }
}
