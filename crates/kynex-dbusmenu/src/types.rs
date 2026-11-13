//! Validated strongly-typed menu structures and strict enum definitions.

/// Structural kind of menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum MenuItemKind {
    /// Standard interactive or container menu item.
    #[default]
    Normal = 0,
    /// Visual separator line between items.
    Separator = 1,
}

impl MenuItemKind {
    /// Converts the enum to its primitive u32 representation.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Checkable toggle decoration kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum ToggleKind {
    /// No toggle mark.
    #[default]
    None = 0,
    /// Checkbox mark.
    Checkbox = 1,
    /// Radio mark.
    Radio = 2,
}

impl ToggleKind {
    /// Converts the enum to its primitive u32 representation.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// State of a toggle item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum ToggleState {
    /// Unknown or indeterminate state.
    Unknown = -1,
    /// Unchecked state.
    #[default]
    Unchecked = 0,
    /// Checked state.
    Checked = 1,
}

impl ToggleState {
    /// Converts the enum to its primitive i32 representation.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

/// Visual disposition of a menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum MenuDisposition {
    /// Standard neutral presentation.
    #[default]
    Normal = 0,
    /// Informational entry (e.g. status details).
    Informative = 1,
    /// Warning prompt.
    Warning = 2,
    /// Urgent alert entry.
    Alert = 3,
}

impl MenuDisposition {
    /// Converts the enum to its primitive u32 representation.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Fully sanitized and validated menu item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMenuItem {
    /// Unique non-negative menu item ID.
    pub id: i32,
    /// Parent menu item ID (0 indicates top-level under the root menu).
    pub parent: i32,
    /// Sanitized display label.
    pub label: String,
    /// Whether the item is interactive.
    pub enabled: bool,
    /// Whether the item is visible.
    pub visible: bool,
    /// Structural kind (normal or separator).
    pub kind: MenuItemKind,
    /// Toggle mark type (none, checkbox, radio).
    pub toggle: ToggleKind,
    /// Toggle state (unknown, unchecked, checked).
    pub toggle_state: ToggleState,
    /// Sanitized Freedesktop icon theme name.
    pub icon_name: String,
    /// Visual disposition styling.
    pub disposition: MenuDisposition,
    /// Sanitized keyboard shortcut representation.
    pub shortcut: String,
    /// Whether this item has sub-items.
    pub has_submenu: bool,
}

/// Fully validated menu layout snapshot with revision number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMenuLayout {
    /// Monotonically increasing layout revision.
    pub revision: u32,
    /// Ordered flat list of all visible menu items.
    pub items: Vec<ValidatedMenuItem>,
}

impl ValidatedMenuLayout {
    /// Finds a menu item by ID in the validated layout.
    #[must_use]
    pub fn find_item(&self, id: i32) -> Option<&ValidatedMenuItem> {
        self.items.iter().find(|item| item.id == id)
    }
}
