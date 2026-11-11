//! Resource limits and security boundaries for the system tray subsystem.

/// Maximum number of active StatusNotifierItem instances per unique D-Bus sender.
pub const MAX_ITEMS_PER_SENDER: usize = 4;

/// Maximum total active StatusNotifierItem instances tracked across the entire session.
pub const MAX_TOTAL_ITEMS: usize = 64;

/// Maximum character length for sanitized item identifiers.
pub const MAX_ID_CHARS: usize = 64;

/// Maximum character length for sanitized item titles.
pub const MAX_TITLE_CHARS: usize = 128;

/// Maximum character length for sanitized tooltip titles.
pub const MAX_TOOLTIP_TITLE_CHARS: usize = 128;

/// Maximum character length for sanitized tooltip body texts.
pub const MAX_TOOLTIP_TEXT_CHARS: usize = 512;

/// Maximum character length for icon names.
pub const MAX_ICON_NAME_CHARS: usize = 128;

/// Maximum allowed width and height for tray icon pixmaps (64x64).
pub const MAX_PIXMAP_DIMENSION: i32 = 64;

/// Maximum allowed byte buffer size for raw icon pixmaps (64 KiB).
pub const MAX_PIXMAP_BYTES: usize = 64 * 1024;

/// Maximum duration in seconds to wait for asynchronous property calls on external items.
pub const PROPERTY_TIMEOUT_SECS: u64 = 2;

/// Maximum number of property refresh updates permitted per item per second.
pub const MAX_REFRESHES_PER_SEC: u32 = 10;
