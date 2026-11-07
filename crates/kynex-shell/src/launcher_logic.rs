//! Pure-Rust logic for the launcher overlay: selection, sequence-based latest-wins,
//! and untrusted display text sanitization.

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
#[derive(Debug, Default)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(SelectionState::select_next(2, 3), 0); // wrap to 0

        assert_eq!(SelectionState::select_prev(2, 3), 1);
        assert_eq!(SelectionState::select_prev(1, 3), 0);
        assert_eq!(SelectionState::select_prev(0, 3), 2); // wrap to 2
    }

    #[test]
    fn test_latest_wins_sequence_ordering() {
        let mut tracker = LatestWinsTracker::new();
        let q1 = tracker.next_seq();
        let q2 = tracker.next_seq();
        let q3 = tracker.next_seq();

        assert_eq!(q1, 1);
        assert_eq!(q2, 2);
        assert_eq!(q3, 3);

        // Out-of-order delivery: q2 arrives first
        assert!(tracker.should_apply(q2));
        assert_eq!(tracker.last_applied_seq(), 2);

        // Stale q1 arrives later -> must be rejected
        assert!(!tracker.should_apply(q1));
        assert_eq!(tracker.last_applied_seq(), 2);

        // Newer q3 arrives -> must be accepted
        assert!(tracker.should_apply(q3));
        assert_eq!(tracker.last_applied_seq(), 3);
    }

    #[test]
    fn test_sanitize_display_text_clean() {
        let input = "Firefox Web Browser";
        let out = sanitize_display_text(input, 128);
        assert_eq!(out, "Firefox Web Browser");
    }

    #[test]
    fn test_sanitize_display_text_control_characters() {
        let input = "Konsole\0\r\n\x08\x1b[31mTerminal\x7F";
        let out = sanitize_display_text(input, 128);
        assert_eq!(out, "Konsole[31mTerminal");
    }

    #[test]
    fn test_sanitize_display_text_bidi_override_characters() {
        // U+202E (Right-to-Left Override) attempting file extension spoofing
        let hostile = "Important_Doc\u{202E}fdp.exe\u{202C}";
        let out = sanitize_display_text(hostile, 128);
        assert_eq!(out, "Important_Docfdp.exe");
        assert!(!out.contains('\u{202E}'));
        assert!(!out.contains('\u{202C}'));

        // Additional bidi isolates (U+2066..=U+2069)
        let bidi_isolates = "\u{2066}Safe\u{2069}\u{2067}App\u{2068}";
        let out_iso = sanitize_display_text(bidi_isolates, 128);
        assert_eq!(out_iso, "SafeApp");
    }

    #[test]
    fn test_sanitize_display_text_html_and_injection() {
        let html = "<script>alert('xss')</script><b>Bold App</b>";
        let out = sanitize_display_text(html, 128);
        // Sanitizer strips control chars, textFormat: Text.PlainText ensures literal rendering
        assert_eq!(out, "<script>alert('xss')</script><b>Bold App</b>");

        let img = r#"<img src="http://127.0.0.1/leak" onerror="alert(1)">"#;
        let out_img = sanitize_display_text(img, 128);
        assert_eq!(out_img, img);
    }

    #[test]
    fn test_sanitize_display_text_length_capping() {
        let huge = "A".repeat(10_000);
        let out = sanitize_display_text(&huge, 128);
        assert_eq!(out.chars().count(), 128);
    }

    #[test]
    fn test_sanitize_display_text_empty_and_whitespace() {
        assert_eq!(sanitize_display_text("", 128), "");
        assert_eq!(sanitize_display_text("   \t  \n  ", 128), "");
    }

    #[test]
    fn test_sanitize_app_item_fallback() {
        let item = AppItem {
            id: "org.kynex.calculator.desktop".to_string(),
            name: "   \0\t  ".to_string(),
            generic_name: "Math Calculator".to_string(),
            comment: "Simple Calculator".to_string(),
            icon_name: "accessories-calculator".to_string(),
            categories: vec!["Utility".to_string()],
        };

        let display = sanitize_app_item(&item);
        assert_eq!(display.name, "org.kynex.calculator");
        assert_eq!(display.generic_name, "Math Calculator");
        assert_eq!(display.letter_badge, "O");
    }

    #[test]
    fn test_compute_letter_badge() {
        assert_eq!(compute_letter_badge("Firefox", "firefox.desktop"), "F");
        assert_eq!(compute_letter_badge("1Password", "1password.desktop"), "1");
        assert_eq!(compute_letter_badge("  éclipse", "eclipse.desktop"), "É");
        assert_eq!(compute_letter_badge("---", "app.desktop"), "A");
        assert_eq!(compute_letter_badge("---", "---"), "?");
    }
}
