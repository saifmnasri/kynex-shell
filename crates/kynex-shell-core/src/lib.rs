//! Kynex Shell Core library.
//!
//! Pure-Rust business logic for the shell presentation layer, decoupled from Qt Quick / CXX-Qt.

pub mod launcher;
pub mod matching;
pub mod taskbar;
pub mod tray;

pub use launcher::{
    compute_letter_badge, sanitize_app_item, sanitize_display_text, sanitize_query, AppDisplayItem,
    LatestWinsTracker, LauncherCore, SelectionState, MAX_COMMENT_LEN, MAX_GENERIC_NAME_LEN,
    MAX_LAUNCHER_RESULTS, MAX_NAME_LEN, MAX_QUERY_LEN,
};
pub use matching::{
    extract_reverse_dns_leaf, match_window_to_app, normalize_for_comparison, strip_desktop_suffix,
    AppCandidate,
};
pub use taskbar::{TaskbarAction, TaskbarCore, TaskbarEntry};
pub use tray::{compute_letter_badge as compute_tray_letter_badge, TrayUiItem, TrayUiState};
