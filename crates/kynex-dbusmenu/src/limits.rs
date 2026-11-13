//! Resource bounds and limit constants for dbusmenu layout parsing and validation.

/// Maximum menu nesting depth (root is depth 0).
pub const MAX_MENU_DEPTH: usize = 6;

/// Maximum total items allowed in a single menu layout.
pub const MAX_TOTAL_ITEMS: usize = 500;

/// Maximum number of direct child nodes allowed per parent node.
pub const MAX_CHILDREN_PER_NODE: usize = 100;

/// Maximum length of a sanitized menu item label in characters.
pub const MAX_LABEL_LEN: usize = 256;

/// Maximum length of a formatted shortcut key string in characters.
pub const MAX_SHORTCUT_LEN: usize = 64;

/// Maximum length of a Freedesktop icon theme name in characters.
pub const MAX_ICON_NAME_LEN: usize = 128;

/// Per-item refresh rate limit (max updates per second).
pub const REFRESH_RATE_LIMIT_PER_SEC: u32 = 10;

/// Timeout in seconds for remote `com.canonical.dbusmenu` D-Bus calls.
pub const MENU_TIMEOUT_SECS: u64 = 2;

/// Maximum number of concurrent in-flight menu fetch tasks.
pub const MAX_CONCURRENT_FETCHES: usize = 8;
