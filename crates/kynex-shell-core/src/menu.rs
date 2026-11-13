//! Pure-Rust business logic for shell menus (dbusmenu presentation and navigation).
//!
//! Enforces:
//! - Pure, deterministic coordinate clamping and flipping for menu popup surfaces across all four panel edges.
//! - Multi-monitor support with negative origins and `HiDPI` scaling.
//! - Keyboard navigation state tracking: skipping separators and disabled items, wrap-around.
//! - Submenu stack traversal (drill-down and pop-back).
//! - Role definitions and mappings for Qt Quick / QML presentation.

use crate::coordinates::{OutputGeometry, PanelSize, Point, SurfaceAnchors};
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

/// Strongly-typed role value returned from menu items.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuRoleValue {
    /// String text value.
    Text(String),
    /// Boolean state value.
    Bool(bool),
    /// Integer numeric value.
    Int(i32),
}

/// Retrieves the typed value of a menu item for a given role ID.
#[allow(clippy::cast_possible_wrap)]
#[must_use]
pub fn menu_item_role_value(item: &MenuItemDto, role: i32) -> Option<MenuRoleValue> {
    match role {
        ROLE_MENU_ID => Some(MenuRoleValue::Int(item.id)),
        ROLE_MENU_PARENT_ID => Some(MenuRoleValue::Int(item.parent)),
        ROLE_MENU_LABEL => Some(MenuRoleValue::Text(item.label.clone())),
        ROLE_MENU_ENABLED => Some(MenuRoleValue::Bool(item.enabled)),
        ROLE_MENU_VISIBLE => Some(MenuRoleValue::Bool(item.visible)),
        ROLE_MENU_KIND => Some(MenuRoleValue::Int(item.kind as i32)),
        ROLE_MENU_TOGGLE => Some(MenuRoleValue::Int(item.toggle as i32)),
        ROLE_MENU_TOGGLE_STATE => Some(MenuRoleValue::Int(item.toggle_state)),
        ROLE_MENU_ICON_NAME => Some(MenuRoleValue::Text(item.icon_name.clone())),
        ROLE_MENU_DISPOSITION => Some(MenuRoleValue::Int(item.disposition as i32)),
        ROLE_MENU_SHORTCUT => Some(MenuRoleValue::Text(item.shortcut.clone())),
        ROLE_MENU_HAS_SUBMENU => Some(MenuRoleValue::Bool(item.has_submenu)),
        _ => None,
    }
}

/// Screen edge on which a panel is docked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelEdge {
    /// Anchored to the top edge of the display.
    Top,
    /// Anchored to the bottom edge of the display.
    Bottom,
    /// Anchored to the left edge of the display.
    Left,
    /// Anchored to the right edge of the display.
    Right,
}

/// Derives the panel edge from surface anchors.
///
/// Priority rule:
/// - `bottom && !top` => `Bottom`
/// - `top && !bottom` => `Top`
/// - `left && !right` => `Left`
/// - `right && !left` => `Right`
/// - anything else => `Bottom` (documented fallback for unanchored or ambiguous configurations).
#[must_use]
pub fn derive_panel_edge(anchors: SurfaceAnchors) -> PanelEdge {
    if anchors.bottom && !anchors.top {
        PanelEdge::Bottom
    } else if anchors.top && !anchors.bottom {
        PanelEdge::Top
    } else if anchors.left && !anchors.right {
        PanelEdge::Left
    } else if anchors.right && !anchors.left {
        PanelEdge::Right
    } else {
        PanelEdge::Bottom
    }
}

/// Computes the clamped and flipped popup position for a menu relative to its panel anchor.
///
/// Returns coordinates relative to the output window top-left `(0.0, 0.0)`.
///
/// # Placement & Flipping Rules:
/// - Derived edge drives placement:
///   - `Bottom`: opens ABOVE the panel (`y = anchor_y - menu_height`); flips horizontally if clipping right edge.
///   - `Top`: opens BELOW the panel (`y = anchor_y + anchor_height`); flips horizontally if clipping right edge.
///   - `Left`: opens to the RIGHT beside the icon (`x = anchor_x + anchor_width`); flips vertically (bottom-aligning to icon) if clipping bottom edge.
///   - `Right`: opens to the LEFT beside the icon (`x = anchor_x - menu_width`); flips vertically (bottom-aligning to icon) if clipping bottom edge.
/// - Clamping: result coordinates are strictly clamped inside `[0.0, (screen.width - menu_width).max(0.0)]`
///   and `[0.0, (screen.height - menu_height).max(0.0)]`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn compute_menu_placement(
    anchors: SurfaceAnchors,
    panel: PanelSize,
    anchor_local: Point,
    anchor_width: f64,
    anchor_height: f64,
    menu_width: f64,
    menu_height: f64,
    screen: OutputGeometry,
) -> Point {
    let edge = derive_panel_edge(anchors);

    // 1. Calculate panel origin relative to output (0, 0)
    let panel_rel_x = match edge {
        PanelEdge::Right => (screen.width - panel.width).max(0.0),
        _ => 0.0,
    };

    let panel_rel_y = match edge {
        PanelEdge::Bottom => (screen.height - panel.height).max(0.0),
        _ => 0.0,
    };

    let anchor_x = panel_rel_x + anchor_local.x;
    let anchor_y = panel_rel_y + anchor_local.y;

    let (mut x, mut y) = match edge {
        PanelEdge::Bottom => {
            // Bottom panel: opens ABOVE
            let y_open = anchor_y - menu_height;
            let y_pos = if y_open < 0.0 && (anchor_y + anchor_height + menu_height <= screen.height)
            {
                anchor_y + anchor_height
            } else {
                y_open
            };
            (anchor_x, y_pos)
        }
        PanelEdge::Top => {
            // Top panel: opens BELOW
            let y_open = anchor_y + anchor_height;
            let y_pos = if (y_open + menu_height > screen.height) && (anchor_y - menu_height >= 0.0)
            {
                anchor_y - menu_height
            } else {
                y_open
            };
            (anchor_x, y_pos)
        }
        PanelEdge::Left => {
            // Left panel: opens to the RIGHT beside the icon
            let x_open = anchor_x + anchor_width;
            let x_pos = if (x_open + menu_width > screen.width) && (anchor_x - menu_width >= 0.0) {
                anchor_x - menu_width
            } else {
                x_open
            };
            (x_pos, anchor_y)
        }
        PanelEdge::Right => {
            // Right panel: opens to the LEFT beside the icon
            let x_open = anchor_x - menu_width;
            let x_pos = if (x_open < 0.0) && (anchor_x + anchor_width + menu_width <= screen.width)
            {
                anchor_x + anchor_width
            } else {
                x_open
            };
            (x_pos, anchor_y)
        }
    };

    // Horizontal flipping for top/bottom panels
    if matches!(edge, PanelEdge::Top | PanelEdge::Bottom) && (x + menu_width > screen.width) {
        x = anchor_x + anchor_width - menu_width;
    }

    // Vertical flipping for left/right panels (bottom-aligning to the icon when it would overflow)
    if matches!(edge, PanelEdge::Left | PanelEdge::Right) && (y + menu_height > screen.height) {
        y = anchor_y + anchor_height - menu_height;
    }

    // Strict clamping within output boundaries [0.0, max]
    let max_x = (screen.width - menu_width).max(0.0);
    let max_y = (screen.height - menu_height).max(0.0);

    Point::new(x.clamp(0.0, max_x), y.clamp(0.0, max_y))
}

/// Computes menu popup position in global compositor coordinates (taking `screen.x` and `screen.y` into account).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn compute_menu_placement_global(
    anchors: SurfaceAnchors,
    panel: PanelSize,
    anchor_local: Point,
    anchor_width: f64,
    anchor_height: f64,
    menu_width: f64,
    menu_height: f64,
    screen: OutputGeometry,
) -> Point {
    let local = compute_menu_placement(
        anchors,
        panel,
        anchor_local,
        anchor_width,
        anchor_height,
        menu_width,
        menu_height,
        screen,
    );
    Point::new(screen.x + local.x, screen.y + local.y)
}

/// Computes the clamped and flipped popup position for a menu relative to its anchor (legacy adapter).
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

    // Vertical calculation
    let anchor_mid_y = anchor.y + (anchor_height / 2.0);
    let screen_mid_y = screen_top + (screen.height / 2.0);

    let mut y = if anchor_mid_y >= screen_mid_y {
        anchor.y - menu_height
    } else {
        anchor.y + anchor_height
    };

    if y < screen_top && (anchor.y + anchor_height + menu_height <= screen_bottom) {
        y = anchor.y + anchor_height;
    } else if (y + menu_height > screen_bottom) && (anchor.y - menu_height >= screen_top) {
        y = anchor.y - menu_height;
    }

    // Horizontal calculation
    let mut x = anchor.x;
    if x + menu_width > screen_right {
        x = anchor.x + anchor_width - menu_width;
    }

    let max_x = (screen_right - menu_width).max(screen_left);
    let max_y = (screen_bottom - menu_height).max(screen_top);

    Point::new(x.clamp(screen_left, max_x), y.clamp(screen_top, max_y))
}

/// Latency measurement tracker for menu rendering performance.
#[derive(Debug, Clone, Default)]
pub struct MenuLatencyTracker {
    last_open_time: Option<std::time::Instant>,
    first_frame_pending: bool,
}

impl MenuLatencyTracker {
    /// Creates a new latency tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the start of a menu opening operation.
    pub fn mark_open_requested(&mut self) {
        self.last_open_time = Some(std::time::Instant::now());
        self.first_frame_pending = true;
    }

    /// Records the first frame rendered after opening, returning elapsed latency.
    pub fn record_first_frame(&mut self) -> Option<std::time::Duration> {
        if self.first_frame_pending {
            self.first_frame_pending = false;
            self.last_open_time.map(|t| t.elapsed())
        } else {
            None
        }
    }

    /// Returns whether a first-frame recording is currently pending.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.first_frame_pending
    }
}

/// Handles D-Bus name owner changed signals for the tray menu subsystem.
/// When the daemon disconnects (`new_owner` is None or empty), clears menu state.
/// Returns `true` if state was reset.
pub fn handle_menu_name_owner_changed(state: &mut MenuCoreState, new_owner: Option<&str>) -> bool {
    let is_disconnected = match new_owner {
        None => true,
        Some(s) => s.is_empty(),
    };
    if is_disconnected {
        state.clear();
        true
    } else {
        false
    }
}

/// Checks if a `MenuChanged` signal matches the active menu item and should trigger refresh.
#[must_use]
pub fn should_update_on_menu_changed(
    active_item_id: &str,
    signal_item_id: &str,
    is_visible: bool,
) -> bool {
    is_visible && !active_item_id.is_empty() && active_item_id == signal_item_id
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
    /// Monotonic revision of the menu layout currently loaded.
    pub revision: u32,
}

impl MenuCoreState {
    /// Creates a new empty menu state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            nav_stack: vec![0],
            selected_index: None,
            revision: 0,
        }
    }

    /// Sets the full list of items for the menu, resetting navigation to root.
    pub fn set_items(&mut self, items: Vec<MenuItemDto>) {
        self.items = items;
        self.nav_stack = vec![0];
        self.selected_index = self.find_first_selectable();
    }

    /// Sets the full list of items along with layout revision.
    pub fn set_items_with_revision(&mut self, items: Vec<MenuItemDto>, revision: u32) {
        self.items = items;
        self.nav_stack = vec![0];
        self.selected_index = self.find_first_selectable();
        self.revision = revision;
    }

    /// Clears the items and navigation state.
    pub fn clear(&mut self) {
        self.items.clear();
        self.nav_stack = vec![0];
        self.selected_index = None;
        self.revision = 0;
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

    /// Returns the total item count in the menu.
    #[must_use]
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Currently selected index in the active list.
    #[must_use]
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    /// Sets selected index directly.
    pub fn set_selected_index(&mut self, idx: Option<usize>) {
        self.selected_index = idx;
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
    fn test_placement_all_four_edges() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let menu_w = 200.0;
        let menu_h = 300.0;

        // 1. Bottom panel: opens ABOVE
        let bottom_panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        let bottom_anchors = SurfaceAnchors::bottom_panel();
        let local_bottom = Point::new(100.0, 8.0); // y=8 inside 48px panel
        let pos_bottom = compute_menu_placement(
            bottom_anchors,
            bottom_panel,
            local_bottom,
            32.0,
            32.0,
            menu_w,
            menu_h,
            screen,
        );
        // Anchor y in output = 1080 - 48 + 8 = 1040.
        // Opens above: y = 1040 - 300 = 740.
        assert_eq!(pos_bottom.y, 740.0, "Bottom panel menu must open above");
        assert_eq!(pos_bottom.x, 100.0);

        // 2. Top panel: opens BELOW
        let top_panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        let top_anchors = SurfaceAnchors::top_panel();
        let local_top = Point::new(100.0, 8.0);
        let pos_top = compute_menu_placement(
            top_anchors,
            top_panel,
            local_top,
            32.0,
            32.0,
            menu_w,
            menu_h,
            screen,
        );
        // Anchor y in output = 8. Opens below: y = 8 + 32 = 40.
        assert_eq!(pos_top.y, 40.0, "Top panel menu must open below");
        assert_eq!(pos_top.x, 100.0);

        // 3. Left panel: opens to the RIGHT
        let left_panel = PanelSize {
            width: 48.0,
            height: 1080.0,
        };
        let left_anchors = SurfaceAnchors::left_panel();
        let local_left = Point::new(8.0, 200.0);
        let pos_left = compute_menu_placement(
            left_anchors,
            left_panel,
            local_left,
            32.0,
            32.0,
            menu_w,
            menu_h,
            screen,
        );
        // Anchor x in output = 8. Opens right: x = 8 + 32 = 40.
        assert_eq!(pos_left.x, 40.0, "Left panel menu must open to the right");
        assert_eq!(pos_left.y, 200.0);

        // 4. Right panel: opens to the LEFT
        let right_panel = PanelSize {
            width: 48.0,
            height: 1080.0,
        };
        let right_anchors = SurfaceAnchors::right_panel();
        let local_right = Point::new(8.0, 200.0);
        let pos_right = compute_menu_placement(
            right_anchors,
            right_panel,
            local_right,
            32.0,
            32.0,
            menu_w,
            menu_h,
            screen,
        );
        // Anchor x in output = 1920 - 48 + 8 = 1880.
        // Opens left: x = 1880 - 200 = 1680.
        assert_eq!(
            pos_right.x, 1680.0,
            "Right panel menu must open to the left"
        );
        assert_eq!(pos_right.y, 200.0);
    }

    #[test]
    fn test_placement_secondary_monitor_negative_x() {
        let screen = OutputGeometry {
            x: -1920.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        let anchors = SurfaceAnchors::bottom_panel();
        let local = Point::new(100.0, 8.0);

        // Local coordinates inside output window:
        let local_pos =
            compute_menu_placement(anchors, panel, local, 32.0, 32.0, 200.0, 300.0, screen);
        assert_eq!(local_pos.x, 100.0);
        assert_eq!(local_pos.y, 740.0);

        // Global compositor coordinates:
        let global_pos =
            compute_menu_placement_global(anchors, panel, local, 32.0, 32.0, 200.0, 300.0, screen);
        assert_eq!(global_pos.x, -1820.0);
        assert_eq!(global_pos.y, 740.0);
    }

    #[test]
    fn test_placement_scaled_output() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 2.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        let pos = compute_menu_placement(
            SurfaceAnchors::bottom_panel(),
            panel,
            Point::new(500.0, 8.0),
            32.0,
            32.0,
            200.0,
            300.0,
            screen,
        );
        assert_eq!(pos.x, 500.0);
        assert_eq!(pos.y, 740.0);
    }

    #[test]
    fn test_placement_menu_taller_than_screen() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        // Menu height 1500 > screen height 1080
        let pos = compute_menu_placement(
            SurfaceAnchors::bottom_panel(),
            panel,
            Point::new(100.0, 8.0),
            32.0,
            32.0,
            200.0,
            1500.0,
            screen,
        );
        // Clamped inside output: y must clamp to 0.0
        assert_eq!(pos.y, 0.0, "Oversized menu height must clamp to 0.0");
        assert_eq!(pos.x, 100.0);
    }

    #[test]
    fn test_placement_500_item_menu() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        // 500 items * 32px = 16,000 px
        let pos = compute_menu_placement(
            SurfaceAnchors::bottom_panel(),
            panel,
            Point::new(100.0, 8.0),
            32.0,
            32.0,
            250.0,
            16000.0,
            screen,
        );
        assert_eq!(
            pos.y, 0.0,
            "500-item menu must clamp cleanly without overflow"
        );
        assert_eq!(pos.x, 100.0);
    }

    #[test]
    fn test_placement_horizontal_flip_at_right_edge() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        // Icon at x = 1850. Menu width 200. 1850 + 200 = 2050 > 1920.
        let pos = compute_menu_placement(
            SurfaceAnchors::bottom_panel(),
            panel,
            Point::new(1850.0, 8.0),
            32.0,
            32.0,
            200.0,
            300.0,
            screen,
        );
        // Flipped left: 1850 + 32 - 200 = 1682.
        assert_eq!(pos.x, 1682.0, "Menu near right edge must flip horizontally");
        assert!(
            pos.x + 200.0 <= 1920.0,
            "Flipped menu must remain inside output"
        );
    }

    #[test]
    fn test_placement_vertical_flip_at_screen_edge() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 48.0,
            height: 1080.0,
        };
        // Left panel (top and bottom both true) with icon near bottom edge (y=1000)
        let pos_left = compute_menu_placement(
            SurfaceAnchors::left_panel(),
            panel,
            Point::new(8.0, 1000.0),
            32.0,
            32.0,
            200.0,
            350.0,
            screen,
        );
        // Flipped Y: 1000 + 32 - 350 = 682
        assert_eq!(
            pos_left.y, 682.0,
            "Left panel menu near bottom edge must flip vertically (bottom-align to icon)"
        );
        assert_eq!(pos_left.x, 40.0);

        // Right panel (top and bottom both true) with icon near bottom edge (y=1000)
        let pos_right = compute_menu_placement(
            SurfaceAnchors::right_panel(),
            panel,
            Point::new(8.0, 1000.0),
            32.0,
            32.0,
            200.0,
            350.0,
            screen,
        );
        assert_eq!(
            pos_right.y, 682.0,
            "Right panel menu near bottom edge must flip vertically (bottom-align to icon)"
        );
        assert_eq!(pos_right.x, 1680.0);
    }

    #[test]
    fn test_mutant_placement_edge_derivation() {
        assert_eq!(
            derive_panel_edge(SurfaceAnchors::left_panel()),
            PanelEdge::Left
        );
        assert_eq!(
            derive_panel_edge(SurfaceAnchors::right_panel()),
            PanelEdge::Right
        );
        assert_eq!(
            derive_panel_edge(SurfaceAnchors::top_panel()),
            PanelEdge::Top
        );
        assert_eq!(
            derive_panel_edge(SurfaceAnchors::bottom_panel()),
            PanelEdge::Bottom
        );
        // Fallback for unanchored or ambiguous configurations
        assert_eq!(
            derive_panel_edge(SurfaceAnchors::default()),
            PanelEdge::Bottom
        );

        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel_v = PanelSize {
            width: 48.0,
            height: 1080.0,
        };
        let pos_left = compute_menu_placement(
            SurfaceAnchors::left_panel(),
            panel_v,
            Point::new(8.0, 200.0),
            32.0,
            32.0,
            200.0,
            300.0,
            screen,
        );
        assert_eq!(
            pos_left.x, 40.0,
            "Mutant killed: Left panel menu opens to the right of icon (8 + 32 = 40)"
        );

        let pos_right = compute_menu_placement(
            SurfaceAnchors::right_panel(),
            panel_v,
            Point::new(8.0, 200.0),
            32.0,
            32.0,
            200.0,
            300.0,
            screen,
        );
        assert_eq!(
            pos_right.x, 1680.0,
            "Mutant killed: Right panel menu opens to the left of icon (1880 - 200 = 1680)"
        );
    }

    #[test]
    fn test_mutant_placement_no_vertical_flip() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 48.0,
            height: 1080.0,
        };
        let pos_left = compute_menu_placement(
            SurfaceAnchors::left_panel(),
            panel,
            Point::new(8.0, 1000.0),
            32.0,
            32.0,
            200.0,
            350.0,
            screen,
        );
        assert_eq!(
            pos_left.y, 682.0,
            "Mutant killed: Vertical flip must bottom-align menu to icon at 682.0, not clamp at 730.0"
        );

        let pos_right = compute_menu_placement(
            SurfaceAnchors::right_panel(),
            panel,
            Point::new(8.0, 1000.0),
            32.0,
            32.0,
            200.0,
            350.0,
            screen,
        );
        assert_eq!(
            pos_right.y, 682.0,
            "Mutant killed: Right panel vertical flip must bottom-align menu to icon at 682.0"
        );
    }

    #[test]
    fn test_mutant_placement_hardcoded_anchors() {
        // If bottom panel anchors were hardcoded to top panel, menu would open below
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        let pos = compute_menu_placement(
            SurfaceAnchors::bottom_panel(),
            panel,
            Point::new(100.0, 8.0),
            32.0,
            32.0,
            200.0,
            300.0,
            screen,
        );
        assert!(
            pos.y < 1040.0,
            "Mutant killed: Bottom panel menu opens ABOVE the panel, not below"
        );
        assert_eq!(pos.y, 740.0);
    }

    #[test]
    fn test_mutant_placement_no_horizontal_flip() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        let pos = compute_menu_placement(
            SurfaceAnchors::bottom_panel(),
            panel,
            Point::new(1850.0, 8.0),
            32.0,
            32.0,
            200.0,
            300.0,
            screen,
        );
        assert!(
            pos.x < 1850.0,
            "Mutant killed: Horizontal flip must move X leftward when clipping right edge"
        );
        assert_eq!(pos.x, 1682.0);
    }

    #[test]
    fn test_mutant_placement_no_clamping() {
        let screen = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 48.0,
        };
        let pos = compute_menu_placement(
            SurfaceAnchors::bottom_panel(),
            panel,
            Point::new(100.0, 8.0),
            32.0,
            32.0,
            200.0,
            2000.0,
            screen,
        );
        assert!(
            pos.y >= 0.0,
            "Mutant killed: Oversized menu must clamp to non-negative Y"
        );
        assert_eq!(pos.y, 0.0);
    }

    #[test]
    fn test_menu_item_role_value_all_roles() {
        let item = MenuItemDto {
            id: 101,
            parent: 0,
            label: "Settings".to_string(),
            enabled: true,
            visible: true,
            kind: 0,
            toggle: 1,
            toggle_state: 1,
            icon_name: "preferences-system".to_string(),
            disposition: 2,
            shortcut: "Ctrl+,".to_string(),
            has_submenu: true,
        };

        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_ID),
            Some(MenuRoleValue::Int(101))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_PARENT_ID),
            Some(MenuRoleValue::Int(0))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_LABEL),
            Some(MenuRoleValue::Text("Settings".to_string()))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_ENABLED),
            Some(MenuRoleValue::Bool(true))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_VISIBLE),
            Some(MenuRoleValue::Bool(true))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_KIND),
            Some(MenuRoleValue::Int(0))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_TOGGLE),
            Some(MenuRoleValue::Int(1))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_TOGGLE_STATE),
            Some(MenuRoleValue::Int(1))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_ICON_NAME),
            Some(MenuRoleValue::Text("preferences-system".to_string()))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_DISPOSITION),
            Some(MenuRoleValue::Int(2))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_SHORTCUT),
            Some(MenuRoleValue::Text("Ctrl+,".to_string()))
        );
        assert_eq!(
            menu_item_role_value(&item, ROLE_MENU_HAS_SUBMENU),
            Some(MenuRoleValue::Bool(true))
        );
        assert_eq!(menu_item_role_value(&item, 9999), None);
    }

    #[test]
    fn test_menu_latency_tracker() {
        let mut tracker = MenuLatencyTracker::new();
        assert!(!tracker.is_pending());
        assert!(tracker.record_first_frame().is_none());

        tracker.mark_open_requested();
        assert!(tracker.is_pending());

        let lat = tracker.record_first_frame();
        assert!(lat.is_some());
        assert!(!tracker.is_pending());
        assert!(tracker.record_first_frame().is_none());
    }

    #[test]
    fn test_handle_menu_name_owner_changed() {
        let mut state = MenuCoreState::new();
        state.set_items(vec![make_test_item(1, 0, "Item 1", true, 0, false)]);
        assert_eq!(state.total_count(), 1);

        // Connected -> no reset
        assert!(!handle_menu_name_owner_changed(&mut state, Some(":1.100")));
        assert_eq!(state.total_count(), 1);

        // Disconnected (empty) -> reset
        assert!(handle_menu_name_owner_changed(&mut state, Some("")));
        assert_eq!(state.total_count(), 0);

        state.set_items(vec![make_test_item(2, 0, "Item 2", true, 0, false)]);
        // Disconnected (None) -> reset
        assert!(handle_menu_name_owner_changed(&mut state, None));
        assert_eq!(state.total_count(), 0);
    }

    #[test]
    fn test_should_update_on_menu_changed() {
        assert!(should_update_on_menu_changed("item1", "item1", true));
        assert!(!should_update_on_menu_changed("item1", "item1", false));
        assert!(!should_update_on_menu_changed("item1", "item2", true));
        assert!(!should_update_on_menu_changed("", "", true));
    }

    #[test]
    fn test_menu_state_navigation_and_revisions() {
        let mut state = MenuCoreState::new();
        assert_eq!(state.revision, 0);

        let items = vec![
            make_test_item(1, 0, "Normal 1", true, 0, false),
            make_test_item(2, 0, "Separator", true, 1, false),
            make_test_item(3, 0, "Disabled", false, 0, false),
            make_test_item(4, 0, "Submenu", true, 0, true),
            make_test_item(5, 4, "Sub Item 1", true, 0, false),
        ];

        state.set_items_with_revision(items, 42);
        assert_eq!(state.revision, 42);
        assert_eq!(state.current_parent_id(), 0);
        assert_eq!(state.active_items().len(), 4);
        assert_eq!(state.selected_index(), Some(0));

        // Navigation
        state.select_next();
        assert_eq!(state.selected_index(), Some(3)); // Skips separator and disabled
        state.select_prev();
        assert_eq!(state.selected_index(), Some(0));

        state.select_last();
        assert_eq!(state.selected_index(), Some(3));
        state.select_first();
        assert_eq!(state.selected_index(), Some(0));

        // Submenu drill-down
        state.select_last(); // selected item 4
        assert_eq!(state.enter_submenu(), Some(4));
        assert_eq!(state.current_parent_id(), 4);
        assert_eq!(state.active_items().len(), 1);
        assert_eq!(state.selected_index(), Some(0));

        // Submenu leave
        assert!(state.leave_submenu());
        assert_eq!(state.current_parent_id(), 0);
        assert_eq!(state.selected_index(), Some(3));

        // Cannot leave root
        assert!(!state.leave_submenu());
    }

    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        prop_compose! {
            fn arb_output_geometry()(
                x in -3840.0..3840.0,
                y in -2160.0..2160.0,
                width in 640.0..3840.0,
                height in 480.0..2160.0,
                scale in prop_oneof![Just(1.0), Just(1.25), Just(1.5), Just(2.0)],
            ) -> OutputGeometry {
                OutputGeometry { x, y, width, height, scale }
            }
        }

        prop_compose! {
            fn arb_anchors()(
                choice in 0..4
            ) -> SurfaceAnchors {
                match choice {
                    0 => SurfaceAnchors::bottom_panel(),
                    1 => SurfaceAnchors::top_panel(),
                    2 => SurfaceAnchors::left_panel(),
                    _ => SurfaceAnchors::right_panel(),
                }
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(500))]

            #[test]
            fn prop_menu_within_screen_when_it_fits(
                screen in arb_output_geometry(),
                anchors in arb_anchors(),
                panel_thickness in 24.0f64..80.0f64,
                icon_dim in 16.0f64..48.0f64,
                menu_w in 50.0f64..400.0f64,
                menu_h in 50.0f64..600.0f64,
            ) {
                let edge = derive_panel_edge(anchors);
                let panel = match edge {
                    PanelEdge::Top | PanelEdge::Bottom => PanelSize {
                        width: screen.width,
                        height: panel_thickness.min(screen.height / 4.0),
                    },
                    PanelEdge::Left | PanelEdge::Right => PanelSize {
                        width: panel_thickness.min(screen.width / 4.0),
                        height: screen.height,
                    },
                };

                let icon_w = icon_dim.min(panel.width);
                let icon_h = icon_dim.min(panel.height);
                let icon_local = Point::new(
                    (panel.width - icon_w) / 2.0,
                    (panel.height - icon_h) / 2.0,
                );

                let pos = compute_menu_placement(
                    anchors,
                    panel,
                    icon_local,
                    icon_w,
                    icon_h,
                    menu_w,
                    menu_h,
                    screen,
                );

                if menu_w <= screen.width {
                    prop_assert!(pos.x >= 0.0, "pos.x ({}) < 0.0", pos.x);
                    prop_assert!(
                        pos.x + menu_w <= screen.width + 1e-4,
                        "pos.x + menu_w ({}) > screen.width ({})",
                        pos.x + menu_w,
                        screen.width
                    );
                }
                if menu_h <= screen.height {
                    prop_assert!(pos.y >= 0.0, "pos.y ({}) < 0.0", pos.y);
                    prop_assert!(
                        pos.y + menu_h <= screen.height + 1e-4,
                        "pos.y + menu_h ({}) > screen.height ({})",
                        pos.y + menu_h,
                        screen.height
                    );
                }

                let global = compute_menu_placement_global(
                    anchors,
                    panel,
                    icon_local,
                    icon_w,
                    icon_h,
                    menu_w,
                    menu_h,
                    screen,
                );
                prop_assert!((global.x - (screen.x + pos.x)).abs() < 1e-4);
                prop_assert!((global.y - (screen.y + pos.y)).abs() < 1e-4);
            }

            #[test]
            fn prop_menu_never_overlaps_icon_and_avoids_panel_strip(
                screen in arb_output_geometry(),
                anchors in arb_anchors(),
                panel_thickness in 24.0f64..80.0f64,
                icon_pos_pct in 0.05f64..0.95f64,
                icon_dim in 16.0f64..48.0f64,
                menu_w in 50.0f64..300.0f64,
                menu_h in 50.0f64..400.0f64,
                icon_at_inner_edge in proptest::bool::ANY,
            ) {
                let edge = derive_panel_edge(anchors);
                let (panel, icon_local) = match edge {
                    PanelEdge::Top => {
                        let p = PanelSize {
                            width: screen.width,
                            height: panel_thickness.min(screen.height / 4.0),
                        };
                        let i_w = icon_dim.min(p.height);
                        let i_h = icon_dim.min(p.height);
                        let local_y = if icon_at_inner_edge {
                            p.height - i_h
                        } else {
                            (p.height - i_h) * 0.5
                        };
                        let local = Point::new((screen.width - i_w) * icon_pos_pct, local_y);
                        (p, local)
                    }
                    PanelEdge::Bottom => {
                        let p = PanelSize {
                            width: screen.width,
                            height: panel_thickness.min(screen.height / 4.0),
                        };
                        let i_w = icon_dim.min(p.height);
                        let i_h = icon_dim.min(p.height);
                        let local_y = if icon_at_inner_edge {
                            0.0
                        } else {
                            (p.height - i_h) * 0.5
                        };
                        let local = Point::new((screen.width - i_w) * icon_pos_pct, local_y);
                        (p, local)
                    }
                    PanelEdge::Left => {
                        let p = PanelSize {
                            width: panel_thickness.min(screen.width / 4.0),
                            height: screen.height,
                        };
                        let i_w = icon_dim.min(p.width);
                        let i_h = icon_dim.min(p.width);
                        let local_x = if icon_at_inner_edge {
                            p.width - i_w
                        } else {
                            (p.width - i_w) * 0.5
                        };
                        let local = Point::new(local_x, (screen.height - i_h) * icon_pos_pct);
                        (p, local)
                    }
                    PanelEdge::Right => {
                        let p = PanelSize {
                            width: panel_thickness.min(screen.width / 4.0),
                            height: screen.height,
                        };
                        let i_w = icon_dim.min(p.width);
                        let i_h = icon_dim.min(p.width);
                        let local_x = if icon_at_inner_edge {
                            0.0
                        } else {
                            (p.width - i_w) * 0.5
                        };
                        let local = Point::new(local_x, (screen.height - i_h) * icon_pos_pct);
                        (p, local)
                    }
                };

                let icon_w = icon_dim.min(panel.width);
                let icon_h = icon_dim.min(panel.height);

                let pos = compute_menu_placement(
                    anchors,
                    panel,
                    icon_local,
                    icon_w,
                    icon_h,
                    menu_w,
                    menu_h,
                    screen,
                );

                let panel_rel_x = match edge {
                    PanelEdge::Right => (screen.width - panel.width).max(0.0),
                    _ => 0.0,
                };
                let panel_rel_y = match edge {
                    PanelEdge::Bottom => (screen.height - panel.height).max(0.0),
                    _ => 0.0,
                };

                let icon_x = panel_rel_x + icon_local.x;
                let icon_y = panel_rel_y + icon_local.y;

                let overlap = |r1: (f64, f64, f64, f64), r2: (f64, f64, f64, f64)| -> bool {
                    r1.0 < r2.2 - 1e-4 && r1.2 > r2.0 + 1e-4 && r1.1 < r2.3 - 1e-4 && r1.3 > r2.1 + 1e-4
                };

                let menu_rect = (pos.x, pos.y, pos.x + menu_w, pos.y + menu_h);
                let icon_rect = (icon_x, icon_y, icon_x + icon_w, icon_y + icon_h);

                // Invariant 2: Menu never overlaps icon rectangle when there is room outside it
                let fits_beside_icon = match edge {
                    PanelEdge::Top | PanelEdge::Bottom => {
                        icon_y >= menu_h || (screen.height - (icon_y + icon_h) >= menu_h)
                    }
                    PanelEdge::Left | PanelEdge::Right => {
                        icon_x >= menu_w || (screen.width - (icon_x + icon_w) >= menu_w)
                    }
                };
                if fits_beside_icon {
                    prop_assert!(
                        !overlap(menu_rect, icon_rect),
                        "Menu rect {:?} overlaps icon rect {:?}",
                        menu_rect,
                        icon_rect
                    );
                }

                // Invariant 3: Menu never overlaps panel strip when avoidable:
                // (a) When icon is docked at inner edge, menu completely avoids panel strip
                if icon_at_inner_edge {
                    let panel_rect = match edge {
                        PanelEdge::Top => (0.0, 0.0, screen.width, panel.height),
                        PanelEdge::Bottom => (0.0, screen.height - panel.height, screen.width, screen.height),
                        PanelEdge::Left => (0.0, 0.0, panel.width, screen.height),
                        PanelEdge::Right => (screen.width - panel.width, 0.0, screen.width, screen.height),
                    };
                    let avoidable = match edge {
                        PanelEdge::Top | PanelEdge::Bottom => menu_h <= screen.height - panel.height,
                        PanelEdge::Left | PanelEdge::Right => menu_w <= screen.width - panel.width,
                    };
                    if avoidable {
                        prop_assert!(
                            !overlap(menu_rect, panel_rect),
                            "Menu rect {:?} overlaps panel strip {:?} (edge: {:?})",
                            menu_rect,
                            panel_rect,
                            edge
                        );
                    }
                }

                // (b) For all icon configurations, menu never penetrates deeper into panel strip than the icon anchor
                match edge {
                    PanelEdge::Bottom => {
                        if menu_h <= icon_y {
                            prop_assert!(pos.y + menu_h <= icon_y + 1e-4);
                        }
                    }
                    PanelEdge::Top => {
                        if icon_y + icon_h + menu_h <= screen.height {
                            prop_assert!(pos.y >= icon_y + icon_h - 1e-4);
                        }
                    }
                    PanelEdge::Left => {
                        if icon_x + icon_w + menu_w <= screen.width {
                            prop_assert!(pos.x >= icon_x + icon_w - 1e-4);
                        }
                    }
                    PanelEdge::Right => {
                        if menu_w <= icon_x {
                            prop_assert!(pos.x + menu_w <= icon_x + 1e-4);
                        }
                    }
                }
            }

            #[test]
            fn prop_mirrored_geometries_mirror_results(
                screen_w in 800.0f64..2560.0f64,
                screen_h in 600.0f64..1440.0f64,
                panel_thickness in 24.0f64..64.0f64,
                icon_dim in 16.0f64..32.0f64,
                menu_w in 100.0f64..300.0f64,
                menu_h in 100.0f64..400.0f64,
                offset_ratio in 0.05f64..0.95f64,
            ) {
                let screen = OutputGeometry {
                    x: 0.0,
                    y: 0.0,
                    width: screen_w,
                    height: screen_h,
                    scale: 1.0,
                };

                // A. Left vs Right panel horizontal mirroring
                let v_panel = PanelSize {
                    width: panel_thickness,
                    height: screen_h,
                };
                let icon_y = (screen_h - icon_dim) * offset_ratio;
                let left_icon = Point::new((panel_thickness - icon_dim) / 2.0, icon_y);
                let right_icon = Point::new((panel_thickness - icon_dim) / 2.0, icon_y);

                let pos_left = compute_menu_placement(
                    SurfaceAnchors::left_panel(),
                    v_panel,
                    left_icon,
                    icon_dim,
                    icon_dim,
                    menu_w,
                    menu_h,
                    screen,
                );
                let pos_right = compute_menu_placement(
                    SurfaceAnchors::right_panel(),
                    v_panel,
                    right_icon,
                    icon_dim,
                    icon_dim,
                    menu_w,
                    menu_h,
                    screen,
                );

                prop_assert!(
                    (pos_right.x - (screen_w - pos_left.x - menu_w)).abs() < 1e-4,
                    "Left/Right X mirroring: left_x={}, right_x={}, expected={}",
                    pos_left.x, pos_right.x, screen_w - pos_left.x - menu_w
                );
                prop_assert!(
                    (pos_right.y - pos_left.y).abs() < 1e-4,
                    "Left/Right Y invariant: left_y={}, right_y={}",
                    pos_left.y, pos_right.y
                );

                // B. Top vs Bottom panel vertical mirroring
                let h_panel = PanelSize {
                    width: screen_w,
                    height: panel_thickness,
                };
                let icon_x = (screen_w - icon_dim) * offset_ratio;
                let top_icon = Point::new(icon_x, (panel_thickness - icon_dim) / 2.0);
                let bottom_icon = Point::new(icon_x, (panel_thickness - icon_dim) / 2.0);

                let pos_top = compute_menu_placement(
                    SurfaceAnchors::top_panel(),
                    h_panel,
                    top_icon,
                    icon_dim,
                    icon_dim,
                    menu_w,
                    menu_h,
                    screen,
                );
                let pos_bottom = compute_menu_placement(
                    SurfaceAnchors::bottom_panel(),
                    h_panel,
                    bottom_icon,
                    icon_dim,
                    icon_dim,
                    menu_w,
                    menu_h,
                    screen,
                );

                prop_assert!(
                    (pos_bottom.y - (screen_h - pos_top.y - menu_h)).abs() < 1e-4,
                    "Top/Bottom Y mirroring: top_y={}, bottom_y={}, expected={}",
                    pos_top.y, pos_bottom.y, screen_h - pos_top.y - menu_h
                );
                prop_assert!(
                    (pos_bottom.x - pos_top.x).abs() < 1e-4,
                    "Top/Bottom X invariant: top_x={}, bottom_x={}",
                    pos_top.x, pos_bottom.x
                );
            }
        }
    }
}
