//! Pure-Rust business logic for shell menus (dbusmenu presentation and navigation).
//!
//! Enforces:
//! - Pure, deterministic coordinate clamping and flipping for menu popup surfaces.
//! - Keyboard navigation state tracking: skipping separators and disabled items, wrap-around.
//! - Submenu stack traversal (drill-down and pop-back).
//! - Role definitions and mappings for Qt Quick / QML presentation.

use crate::coordinates::{OutputGeometry, Point};
use kynex_ipc::tray::MenuItemDto;

/// Static role IDs for the menu model exposed to QML.
pub const ROLE_MENU_ID: i32 = 300;
/// Role ID for item parent ID.
pub const ROLE_MENU_PARENT_ID: i32 = 301;
/// Role ID for item text label.
pub const ROLE_MENU_LABEL: i32 = 302;
/// Role ID for whether item is interactable.
pub const ROLE_MENU_ENABLED: i32 = 303;
/// Role ID for whether item is displayed.
pub const ROLE_MENU_VISIBLE: i32 = 304;
/// Role ID for item kind (0 = normal, 1 = separator).
pub const ROLE_MENU_KIND: i32 = 305;
/// Role ID for toggle type (0 = none, 1 = checkmark, 2 = radio).
pub const ROLE_MENU_TOGGLE: i32 = 306;
/// Role ID for toggle state (0 = unchecked, 1 = checked, -1 = indeterminate).
pub const ROLE_MENU_TOGGLE_STATE: i32 = 307;
/// Role ID for sanitized icon name.
pub const ROLE_MENU_ICON_NAME: i32 = 308;
/// Role ID for item disposition (0 = normal, 1 = informative, 2 = warning, 3 = alert).
pub const ROLE_MENU_DISPOSITION: i32 = 309;
/// Role ID for shortcut text string.
pub const ROLE_MENU_SHORTCUT: i32 = 310;
/// Role ID for whether item has child submenu.
pub const ROLE_MENU_HAS_SUBMENU: i32 = 311;

/// Role table mapping IDs to QML property names.
pub const MENU_ROLES: &[(i32, &str)] = &[
    (ROLE_MENU_ID, "id"),
    (ROLE_MENU_PARENT_ID, "parentId"),
    (ROLE_MENU_LABEL, "label"),
    (ROLE_MENU_ENABLED, "enabled"),
    (ROLE_MENU_VISIBLE, "visible"),
    (ROLE_MENU_KIND, "kind"),
    (ROLE_MENU_TOGGLE, "toggle"),
    (ROLE_MENU_TOGGLE_STATE, "toggleState"),
    (ROLE_MENU_ICON_NAME, "iconName"),
    (ROLE_MENU_DISPOSITION, "disposition"),
    (ROLE_MENU_SHORTCUT, "shortcut"),
    (ROLE_MENU_HAS_SUBMENU, "hasSubmenu"),
];

/// Returns the static role names for the menu model.
#[must_use]
pub fn menu_role_names() -> &'static [(i32, &'static str)] {
    MENU_ROLES
}

/// Computes the clamped and flipped popup position for a menu relative to its anchor.
///
/// # Geometry Rules:
/// - Vertical: default orientation flips upwards if anchor is below middle of screen;
///   otherwise positions downwards. If placed upwards would clip screen top, flips downwards.
///   If placed downwards would clip screen bottom, flips upwards.
/// - Horizontal: aligns menu's left edge with anchor's left edge (`anchor.x`). If menu
///   extends past screen right edge, aligns menu's right edge with anchor's right edge.
/// - Result is clamped inside `[screen.x, screen.x + screen.width - menu_width]` and
///   `[screen.y, screen.y + screen.height - menu_height]`.
#[must_use]
pub fn compute_menu_popup_geometry(
    anchor: Point,
    anchor_width: f64,
    anchor_height: f64,
    menu_width: f64,
    menu_height: f64,
    screen: OutputGeometry,
) -> Point {
    let screen_left = screen.x;
    let screen_right = screen.x + screen.width;
    let screen_top = screen.y;
    let screen_bottom = screen.y + screen.height;

    // 1. Vertical calculation
    let anchor_mid_y = anchor.y + (anchor_height / 2.0);
    let screen_mid_y = screen_top + (screen.height / 2.0);

    let mut y = if anchor_mid_y >= screen_mid_y {
        // Closer to bottom: prefer popping upwards
        anchor.y - menu_height
    } else {
        // Closer to top: prefer popping downwards
        anchor.y + anchor_height
    };

    // Flip if clipping screen boundary
    if y < screen_top && (anchor.y + anchor_height + menu_height <= screen_bottom) {
        y = anchor.y + anchor_height;
    } else if (y + menu_height > screen_bottom) && (anchor.y - menu_height >= screen_top) {
        y = anchor.y - menu_height;
    }

    // 2. Horizontal calculation
    let mut x = anchor.x;
    if x + menu_width > screen_right {
        // Flip leftwards
        x = anchor.x + anchor_width - menu_width;
    }

    // 3. Strict clamping to output bounds
    let max_x = (screen_right - menu_width).max(screen_left);
    let max_y = (screen_bottom - menu_height).max(screen_top);

    Point::new(x.clamp(screen_left, max_x), y.clamp(screen_top, max_y))
}

/// Pure-Rust state machine for menu navigation and submenu tracking.
#[derive(Debug, Clone, Default)]
pub struct MenuCoreState {
    /// All items in the menu across all levels.
    items: Vec<MenuItemDto>,
    /// Navigation stack: tracks active parent IDs from root (0) downwards.
    nav_stack: Vec<i32>,
    /// Currently selected index in `active_items()`.
    selected_index: Option<usize>,
}

impl MenuCoreState {
    /// Creates a new empty menu state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            nav_stack: vec![0],
            selected_index: None,
        }
    }

    /// Sets the full list of items for the menu, resetting navigation to root.
    pub fn set_items(&mut self, items: Vec<MenuItemDto>) {
        self.items = items;
        self.nav_stack = vec![0];
        self.selected_index = self.find_first_selectable();
    }

    /// Clears the items and navigation state.
    pub fn clear(&mut self) {
        self.items.clear();
        self.nav_stack = vec![0];
        self.selected_index = None;
    }

    /// Current active parent ID (0 for root).
    #[must_use]
    pub fn current_parent_id(&self) -> i32 {
        self.nav_stack.last().copied().unwrap_or(0)
    }

    /// Returns filtered, visible items for the current active parent level.
    #[must_use]
    pub fn active_items(&self) -> Vec<&MenuItemDto> {
        let parent = self.current_parent_id();
        self.items
            .iter()
            .filter(|it| it.parent == parent && it.visible)
            .collect()
    }

    /// Currently selected index in the active list.
    #[must_use]
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    /// Currently selected item in the active list.
    #[must_use]
    pub fn selected_item(&self) -> Option<&MenuItemDto> {
        let idx = self.selected_index?;
        self.active_items().get(idx).copied()
    }

    /// Finds the first selectable (non-separator and enabled) index.
    #[must_use]
    pub fn find_first_selectable(&self) -> Option<usize> {
        let active = self.active_items();
        active.iter().position(|it| it.kind == 0 && it.enabled)
    }

    /// Finds the last selectable (non-separator and enabled) index.
    #[must_use]
    pub fn find_last_selectable(&self) -> Option<usize> {
        let active = self.active_items();
        active.iter().rposition(|it| it.kind == 0 && it.enabled)
    }

    /// Moves selection down (next selectable item), wrapping around.
    pub fn select_next(&mut self) {
        let active = self.active_items();
        if active.is_empty() {
            self.selected_index = None;
            return;
        }

        let len = active.len();
        let start = self.selected_index.map_or(0, |i| (i + 1) % len);

        for offset in 0..len {
            let candidate = (start + offset) % len;
            if active[candidate].kind == 0 && active[candidate].enabled {
                self.selected_index = Some(candidate);
                return;
            }
        }
    }

    /// Moves selection up (previous selectable item), wrapping around.
    pub fn select_prev(&mut self) {
        let active = self.active_items();
        if active.is_empty() {
            self.selected_index = None;
            return;
        }

        let len = active.len();
        let start = self
            .selected_index
            .map_or(len - 1, |i| if i == 0 { len - 1 } else { i - 1 });

        for offset in 0..len {
            let candidate = if start >= offset {
                start - offset
            } else {
                len - (offset - start)
            };
            if active[candidate].kind == 0 && active[candidate].enabled {
                self.selected_index = Some(candidate);
                return;
            }
        }
    }

    /// Selects the first selectable item.
    pub fn select_first(&mut self) {
        self.selected_index = self.find_first_selectable();
    }

    /// Selects the last selectable item.
    pub fn select_last(&mut self) {
        self.selected_index = self.find_last_selectable();
    }

    /// Opens the submenu of the currently selected item if it has one.
    ///
    /// Returns `Some(submenu_id)` on success, or `None` if item cannot open a submenu.
    pub fn enter_submenu(&mut self) -> Option<i32> {
        let item = self.selected_item()?;
        if item.has_submenu && item.enabled {
            let submenu_id = item.id;
            self.nav_stack.push(submenu_id);
            self.selected_index = self.find_first_selectable();
            Some(submenu_id)
        } else {
            None
        }
    }

    /// Closes the current submenu level, popping back up to parent level.
    ///
    /// Returns `true` if a submenu was closed, or `false` if already at root.
    pub fn leave_submenu(&mut self) -> bool {
        if self.nav_stack.len() > 1 {
            let closed_id = self.nav_stack.pop();
            // Restore selection to the item that opened this submenu
            let active = self.active_items();
            self.selected_index =
                closed_id.and_then(|cid| active.iter().position(|it| it.id == cid));
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_item(
        id: i32,
        parent: i32,
        label: &str,
        enabled: bool,
        kind: u32,
        has_submenu: bool,
    ) -> MenuItemDto {
        MenuItemDto {
            id,
            parent,
            label: label.to_string(),
            enabled,
            visible: true,
            kind,
            toggle: 0,
            toggle_state: 0,
            icon_name: String::new(),
            disposition: 0,
            shortcut: String::new(),
            has_submenu,
        }
    }

    #[test]
    fn test_menu_popup_geometry_bottom_panel_flips_up() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        // Anchor at bottom right (e.g. tray icon)
        let anchor = Point::new(1800.0, 1032.0);
        let anchor_w = 32.0;
        let anchor_h = 32.0;
        let menu_w = 200.0;
        let menu_h = 300.0;

        let pos = compute_menu_popup_geometry(anchor, anchor_w, anchor_h, menu_w, menu_h, screen);
        // Popped up: y = 1032 - 300 = 732
        assert_eq!(pos.y, 732.0);
        // Flipped left: 1800 + 32 - 200 = 1632
        assert_eq!(pos.x, 1632.0);
    }

    #[test]
    fn test_menu_popup_geometry_top_panel_pops_down() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let anchor = Point::new(100.0, 10.0);
        let pos = compute_menu_popup_geometry(anchor, 30.0, 30.0, 150.0, 200.0, screen);
        // Popped down: y = 10 + 30 = 40
        assert_eq!(pos.y, 40.0);
        assert_eq!(pos.x, 100.0);
    }

    #[test]
    fn test_menu_popup_geometry_clamping() {
        let screen = OutputGeometry {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
            scale: 1.0,
        };
        // Out of bounds anchor
        let anchor = Point::new(950.0, 750.0);
        let pos = compute_menu_popup_geometry(anchor, 20.0, 20.0, 200.0, 200.0, screen);
        // Clamped to right/bottom: x = 100 + 800 - 200 = 700, y = 100 + 600 - 200 = 500
        assert_eq!(pos.x, 700.0);
        assert_eq!(pos.y, 500.0);
    }

    #[test]
    fn test_menu_navigation_skips_separators_and_disabled() {
        let items = vec![
            make_test_item(1, 0, "Item 1", true, 0, false),
            make_test_item(2, 0, "Sep", true, 1, false), // Separator
            make_test_item(3, 0, "Item 3 (Disabled)", false, 0, false),
            make_test_item(4, 0, "Item 4", true, 0, false),
        ];

        let mut state = MenuCoreState::new();
        state.set_items(items);

        // Initially selected first selectable: index 0 (Item 1)
        assert_eq!(state.selected_index(), Some(0));

        // Next: skips Sep (1) and Disabled (2), selects Item 4 (index 3)
        state.select_next();
        assert_eq!(state.selected_index(), Some(3));

        // Next: wraps around to index 0
        state.select_next();
        assert_eq!(state.selected_index(), Some(0));

        // Prev: wraps around to index 3
        state.select_prev();
        assert_eq!(state.selected_index(), Some(3));
    }

    #[test]
    fn test_submenu_drill_down_and_pop_back() {
        let items = vec![
            make_test_item(1, 0, "Root Item 1", true, 0, false),
            make_test_item(2, 0, "Submenu 1", true, 0, true),
            make_test_item(10, 2, "Sub Item A", true, 0, false),
            make_test_item(11, 2, "Sub Item B", true, 0, false),
        ];

        let mut state = MenuCoreState::new();
        state.set_items(items);

        assert_eq!(state.current_parent_id(), 0);
        assert_eq!(state.active_items().len(), 2);

        // Select Submenu 1 (index 1)
        state.select_next();
        assert_eq!(state.selected_index(), Some(1));

        // Enter submenu
        let sub_id = state.enter_submenu();
        assert_eq!(sub_id, Some(2));
        assert_eq!(state.current_parent_id(), 2);
        assert_eq!(state.active_items().len(), 2);
        assert_eq!(state.selected_index(), Some(0)); // First item in submenu

        // Pop back out
        assert!(state.leave_submenu());
        assert_eq!(state.current_parent_id(), 0);
        assert_eq!(state.selected_index(), Some(1)); // Returned to Submenu 1

        // Popping again at root returns false
        assert!(!state.leave_submenu());
    }

    #[test]
    fn test_menu_roles_table() {
        let roles = menu_role_names();
        assert_eq!(roles.len(), 12);
        assert_eq!(roles[0].1, "id");
        assert_eq!(roles[2].1, "label");
    }
}
