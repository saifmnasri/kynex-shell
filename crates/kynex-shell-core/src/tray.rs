//! Pure-Rust state machine and data model logic for the system tray in `kynex-shell`.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use kynex_ipc::tray::TrayItemInfo;

/// Role ID for item unique identifier.
pub const ROLE_ID: i32 = 257;
/// Role ID for item title.
pub const ROLE_TITLE: i32 = 258;
/// Role ID for item status (`"Passive"`, `"Active"`, `"NeedsAttention"`).
pub const ROLE_STATUS: i32 = 259;
/// Role ID for icon name.
pub const ROLE_ICON_NAME: i32 = 260;
/// Role ID for whether an in-memory pixmap is present.
pub const ROLE_HAS_PIXMAP: i32 = 261;
/// Role ID for pixmap identifier (used with `image://kynex-tray/<id>`).
pub const ROLE_PIXMAP_ID: i32 = 262;
/// Role ID for tooltip headline.
pub const ROLE_TOOLTIP_TITLE: i32 = 263;
/// Role ID for tooltip body text.
pub const ROLE_TOOLTIP_TEXT: i32 = 264;
/// Role ID for whether the item requests attention.
pub const ROLE_IS_ATTENTION: i32 = 265;
/// Role ID for single-character fallback letter badge.
pub const ROLE_LETTER_BADGE: i32 = 266;
/// Role ID for whether item is menu-only.
pub const ROLE_IS_MENU: i32 = 267;
/// Role ID for associated window ID.
pub const ROLE_WINDOW_ID: i32 = 268;
/// Role ID for associated dbusmenu object path.
pub const ROLE_MENU_PATH: i32 = 269;
/// Role ID for whether item exports a dbusmenu.
pub const ROLE_HAS_MENU: i32 = 270;

/// Static table of all tray role IDs and their corresponding QML property names.
pub const TRAY_ROLES: &[(i32, &str)] = &[
    (ROLE_ID, "id"),
    (ROLE_TITLE, "title"),
    (ROLE_STATUS, "status"),
    (ROLE_ICON_NAME, "iconName"),
    (ROLE_HAS_PIXMAP, "hasPixmap"),
    (ROLE_PIXMAP_ID, "pixmapId"),
    (ROLE_TOOLTIP_TITLE, "tooltipTitle"),
    (ROLE_TOOLTIP_TEXT, "tooltipText"),
    (ROLE_IS_ATTENTION, "isAttention"),
    (ROLE_LETTER_BADGE, "letterBadge"),
    (ROLE_IS_MENU, "isMenu"),
    (ROLE_WINDOW_ID, "windowId"),
    (ROLE_MENU_PATH, "menuPath"),
    (ROLE_HAS_MENU, "hasMenu"),
];

/// Returns the static role name mapping table.
#[must_use]
pub fn tray_role_names() -> &'static [(i32, &'static str)] {
    TRAY_ROLES
}

/// Strongly typed value returned for a model role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayRoleValue {
    /// String property.
    Text(String),
    /// Boolean property.
    Bool(bool),
    /// 32-bit signed integer property.
    Int(i32),
}

/// Retrieves the typed value of a role for a given [`TrayUiItem`].
#[must_use]
pub fn tray_item_role_value(item: &TrayUiItem, role: i32) -> Option<TrayRoleValue> {
    match role {
        ROLE_ID => Some(TrayRoleValue::Text(item.id.clone())),
        ROLE_TITLE => Some(TrayRoleValue::Text(item.title.clone())),
        ROLE_STATUS => Some(TrayRoleValue::Text(item.status.clone())),
        ROLE_ICON_NAME => Some(TrayRoleValue::Text(item.icon_name.clone())),
        ROLE_HAS_PIXMAP => Some(TrayRoleValue::Bool(item.has_pixmap)),
        ROLE_PIXMAP_ID => Some(TrayRoleValue::Text(item.pixmap_id.clone())),
        ROLE_TOOLTIP_TITLE => Some(TrayRoleValue::Text(item.tooltip_title.clone())),
        ROLE_TOOLTIP_TEXT => Some(TrayRoleValue::Text(item.tooltip_text.clone())),
        ROLE_IS_ATTENTION => Some(TrayRoleValue::Bool(item.is_attention)),
        ROLE_LETTER_BADGE => Some(TrayRoleValue::Text(item.letter_badge.clone())),
        ROLE_IS_MENU => Some(TrayRoleValue::Bool(item.is_menu)),
        ROLE_WINDOW_ID => Some(TrayRoleValue::Int(item.window_id)),
        ROLE_MENU_PATH => Some(TrayRoleValue::Text(item.menu_path.clone())),
        ROLE_HAS_MENU => Some(TrayRoleValue::Bool(item.has_menu)),
        _ => None,
    }
}

/// Raw tray icon pixel payload: `(width, height, argb32_data)`.
pub type TrayIconData = (i32, i32, Vec<u8>);

/// Thread-safe in-memory pixmap cache for raw tray icons.
#[derive(Debug, Default)]
pub struct TrayPixmapCache {
    map: RwLock<HashMap<String, TrayIconData>>,
}

impl TrayPixmapCache {
    /// Creates a new pixmap cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
        }
    }

    /// Returns a reference to the global singleton cache.
    #[must_use]
    pub fn global() -> &'static Self {
        static INSTANCE: OnceLock<TrayPixmapCache> = OnceLock::new();
        INSTANCE.get_or_init(Self::new)
    }

    /// Updates or inserts a pixmap entry.
    pub fn update(&self, id: &str, width: i32, height: i32, data: &[u8]) {
        if let Ok(mut lock) = self.map.write() {
            if data.is_empty() || width <= 0 || height <= 0 {
                lock.remove(id);
            } else {
                lock.insert(id.to_string(), (width, height, data.to_vec()));
            }
        }
    }

    /// Removes a pixmap entry by identifier.
    pub fn remove(&self, id: &str) {
        if let Ok(mut lock) = self.map.write() {
            lock.remove(id);
        }
    }

    /// Clears all entries in the cache.
    pub fn clear(&self) {
        if let Ok(mut lock) = self.map.write() {
            lock.clear();
        }
    }

    /// Retrieves an entry from the cache.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<(i32, i32, Vec<u8>)> {
        self.map.read().ok()?.get(id).cloned()
    }

    /// Copies cached pixmap bytes into a caller-provided destination slice.
    ///
    /// Returns the number of bytes copied.
    pub fn copy_to_slice(
        &self,
        id: &str,
        out_width: &mut i32,
        out_height: &mut i32,
        out_buf: &mut [u8],
    ) -> usize {
        let Some((w, h, data)) = self.get(id) else {
            return 0;
        };
        *out_width = w;
        *out_height = h;
        let copy_len = data.len().min(out_buf.len());
        out_buf[..copy_len].copy_from_slice(&data[..copy_len]);
        copy_len
    }
}

/// Incoming event for the tray state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// Item was added.
    ItemAdded(Box<TrayItemInfo>),
    /// Item properties were updated.
    ItemChanged(Box<TrayItemInfo>),
    /// Item was removed by identifier.
    ItemRemoved(String),
    /// Daemon left the bus; all items must be cleared.
    DaemonDisconnected,
}

/// Handles an incoming tray event, updating state and pixmap cache.
///
/// Returns `true` if state was modified.
pub fn handle_tray_event(
    state: &mut TrayUiState,
    pixmap_cache: &TrayPixmapCache,
    event: TrayEvent,
) -> bool {
    match event {
        TrayEvent::ItemAdded(info) | TrayEvent::ItemChanged(info) => {
            if !info.icon_pixmap.is_empty()
                && info.icon_pixmap_width > 0
                && info.icon_pixmap_height > 0
            {
                pixmap_cache.update(
                    &info.id,
                    info.icon_pixmap_width,
                    info.icon_pixmap_height,
                    &info.icon_pixmap,
                );
            } else {
                pixmap_cache.remove(&info.id);
            }
            let item = TrayUiItem::from_ipc(&info);
            state.upsert(item);
            true
        }
        TrayEvent::ItemRemoved(id) => {
            pixmap_cache.remove(&id);
            state.remove(&id).is_some()
        }
        TrayEvent::DaemonDisconnected => {
            state.clear();
            pixmap_cache.clear();
            true
        }
    }
}

/// Computes a fallback single-character letter badge for a title or identifier.
#[must_use]
pub fn compute_letter_badge(text: &str) -> String {
    for c in text.chars() {
        if c.is_alphanumeric() {
            return c.to_uppercase().to_string();
        }
    }
    "?".to_string()
}

/// Pure-Rust representation of a system tray item for UI presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct TrayUiItem {
    /// Unique item identifier string (e.g. `":1.42/StatusNotifierItem"`).
    pub id: String,
    /// Sanitized item title.
    pub title: String,
    /// Operational status (`"Passive"`, `"Active"`, `"NeedsAttention"`).
    pub status: String,
    /// Sanitized icon name.
    pub icon_name: String,
    /// Whether an in-memory pixmap is available.
    pub has_pixmap: bool,
    /// Canonical pixmap cache key (`id`).
    pub pixmap_id: String,
    /// Sanitized tooltip headline.
    pub tooltip_title: String,
    /// Sanitized tooltip body description.
    pub tooltip_text: String,
    /// Whether item requested attention.
    pub is_attention: bool,
    /// Fallback single-character badge.
    pub letter_badge: String,
    /// Whether item is a menu-only item.
    pub is_menu: bool,
    /// Window ID if associated with a window.
    pub window_id: i32,
    /// Associated dbusmenu object path if exported.
    pub menu_path: String,
    /// Whether item exports a valid dbusmenu.
    pub has_menu: bool,
    /// Validated RGBA pixmap bytes.
    pub icon_pixmap: Vec<u8>,
    /// Width of `icon_pixmap`.
    pub icon_pixmap_width: i32,
    /// Height of `icon_pixmap`.
    pub icon_pixmap_height: i32,
}

impl TrayUiItem {
    /// Constructs a `TrayUiItem` from D-Bus IPC [`TrayItemInfo`].
    #[must_use]
    pub fn from_ipc(info: &TrayItemInfo) -> Self {
        let has_pixmap = !info.icon_pixmap.is_empty()
            && info.icon_pixmap_width > 0
            && info.icon_pixmap_height > 0;
        let is_attention = info.has_attention || info.status == "NeedsAttention";

        let badge_source = if !info.title.is_empty() {
            &info.title
        } else if !info.icon_name.is_empty() {
            &info.icon_name
        } else {
            &info.id
        };

        Self {
            id: info.id.clone(),
            title: info.title.clone(),
            status: info.status.clone(),
            icon_name: info.icon_name.clone(),
            has_pixmap,
            pixmap_id: info.id.clone(),
            tooltip_title: info.tooltip_title.clone(),
            tooltip_text: info.tooltip_text.clone(),
            is_attention,
            letter_badge: compute_letter_badge(badge_source),
            is_menu: info.is_menu,
            window_id: info.window_id,
            menu_path: info.menu_path.clone(),
            has_menu: !info.menu_path.is_empty(),
            icon_pixmap: info.icon_pixmap.clone(),
            icon_pixmap_width: info.icon_pixmap_width,
            icon_pixmap_height: info.icon_pixmap_height,
        }
    }
}

/// Collection tracking visible tray items for the shell model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayUiState {
    /// Active tray items ordered by arrival.
    pub items: Vec<TrayUiItem>,
}

impl TrayUiState {
    /// Creates an empty collection.
    #[must_use]
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Number of tracked tray items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether collection is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Clears all items.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Inserts or updates an item in the collection.
    ///
    /// Returns `(index, is_update)`.
    pub fn upsert(&mut self, item: TrayUiItem) -> (usize, bool) {
        if let Some(pos) = self.items.iter().position(|it| it.id == item.id) {
            self.items[pos] = item;
            (pos, true)
        } else {
            self.items.push(item);
            (self.items.len() - 1, false)
        }
    }

    /// Removes an item by identifier.
    ///
    /// Returns `Some((index, item))` if found, or `None`.
    pub fn remove(&mut self, id: &str) -> Option<(usize, TrayUiItem)> {
        if let Some(pos) = self.items.iter().position(|it| it.id == id) {
            let item = self.items.remove(pos);
            Some((pos, item))
        } else {
            None
        }
    }

    /// Retrieves an item by identifier.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&TrayUiItem> {
        self.items.iter().find(|it| it.id == id)
    }

    /// Retrieves an item by index.
    #[must_use]
    pub fn get_by_index(&self, index: usize) -> Option<&TrayUiItem> {
        self.items.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_letter_badge() {
        assert_eq!(compute_letter_badge("VLC Media Player"), "V");
        assert_eq!(compute_letter_badge("1Password"), "1");
        assert_eq!(compute_letter_badge("..."), "?");
        assert_eq!(compute_letter_badge(""), "?");
    }

    #[test]
    fn test_ui_item_conversion() {
        let info = TrayItemInfo {
            id: ":1.42/StatusNotifierItem".to_string(),
            title: "Test App".to_string(),
            status: "NeedsAttention".to_string(),
            icon_name: "test-icon".to_string(),
            icon_theme_path: String::new(),
            icon_pixmap: vec![0u8; 16],
            icon_pixmap_width: 2,
            icon_pixmap_height: 2,
            tooltip_title: "ToolTip".to_string(),
            tooltip_text: "Desc".to_string(),
            is_menu: false,
            window_id: 123,
            has_attention: true,
            attention_icon_name: String::new(),
            attention_pixmap: Vec::new(),
            attention_pixmap_width: 0,
            attention_pixmap_height: 0,
            menu_path: "/MenuBar".to_string(),
        };

        let ui_item = TrayUiItem::from_ipc(&info);
        assert_eq!(ui_item.id, ":1.42/StatusNotifierItem");
        assert_eq!(ui_item.title, "Test App");
        assert!(ui_item.has_pixmap);
        assert!(ui_item.is_attention);
        assert_eq!(ui_item.letter_badge, "T");

        // Fallback badge from icon_name
        let mut info_no_title = info.clone();
        info_no_title.title.clear();
        let ui_item2 = TrayUiItem::from_ipc(&info_no_title);
        assert_eq!(ui_item2.letter_badge, "T");

        // Fallback badge from id
        let mut info_only_id = info;
        info_only_id.title.clear();
        info_only_id.icon_name.clear();
        info_only_id.id = ":99.1".to_string();
        let ui_item3 = TrayUiItem::from_ipc(&info_only_id);
        assert_eq!(ui_item3.letter_badge, "9");
    }

    #[test]
    fn test_state_upsert_and_remove() {
        let mut state = TrayUiState::new();
        assert!(state.is_empty());
        assert_eq!(state.len(), 0);

        let item1 = TrayUiItem {
            id: "item1".to_string(),
            title: "Item 1".to_string(),
            status: "Active".to_string(),
            icon_name: "icon-1".to_string(),
            has_pixmap: false,
            pixmap_id: "item1".to_string(),
            tooltip_title: String::new(),
            tooltip_text: String::new(),
            is_attention: false,
            letter_badge: "I".to_string(),
            is_menu: false,
            window_id: 0,
            menu_path: String::new(),
            has_menu: false,
            icon_pixmap: Vec::new(),
            icon_pixmap_width: 0,
            icon_pixmap_height: 0,
        };

        let (idx, is_update) = state.upsert(item1.clone());
        assert_eq!(idx, 0);
        assert!(!is_update);
        assert_eq!(state.len(), 1);
        assert_eq!(
            state.get_by_index(0).map(|it| it.id.as_str()),
            Some("item1")
        );

        // Update item 1
        let mut updated = item1.clone();
        updated.title = "Updated Item 1".to_string();
        let (idx_u, is_u) = state.upsert(updated);
        assert_eq!(idx_u, 0);
        assert!(is_u);
        assert_eq!(
            state.get("item1").map(|it| it.title.as_str()),
            Some("Updated Item 1")
        );

        // Test state clear and default
        let mut def_state = TrayUiState::default();
        def_state.upsert(item1.clone());
        assert_eq!(def_state.len(), 1);
        def_state.clear();
        assert!(def_state.is_empty());

        // Remove item 1
        let rem = state.remove("item1");
        assert!(rem.is_some());
        assert_eq!(state.len(), 0);
        assert!(state.remove("item1").is_none());
        assert!(state.get("item1").is_none());
        assert!(state.get_by_index(0).is_none());
    }

    #[test]
    fn test_role_values() {
        let item = TrayUiItem {
            id: "app.id".to_string(),
            title: "App Title".to_string(),
            status: "Active".to_string(),
            icon_name: "app-icon".to_string(),
            has_pixmap: true,
            pixmap_id: "app.id".to_string(),
            tooltip_title: "Tip Title".to_string(),
            tooltip_text: "Tip Text".to_string(),
            is_attention: true,
            letter_badge: "A".to_string(),
            is_menu: true,
            window_id: 42,
            menu_path: "/MenuBar".to_string(),
            has_menu: true,
            icon_pixmap: vec![0u8; 4],
            icon_pixmap_width: 1,
            icon_pixmap_height: 1,
        };

        assert_eq!(
            tray_item_role_value(&item, ROLE_ID),
            Some(TrayRoleValue::Text("app.id".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_TITLE),
            Some(TrayRoleValue::Text("App Title".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_STATUS),
            Some(TrayRoleValue::Text("Active".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_ICON_NAME),
            Some(TrayRoleValue::Text("app-icon".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_HAS_PIXMAP),
            Some(TrayRoleValue::Bool(true))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_PIXMAP_ID),
            Some(TrayRoleValue::Text("app.id".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_TOOLTIP_TITLE),
            Some(TrayRoleValue::Text("Tip Title".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_TOOLTIP_TEXT),
            Some(TrayRoleValue::Text("Tip Text".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_IS_ATTENTION),
            Some(TrayRoleValue::Bool(true))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_LETTER_BADGE),
            Some(TrayRoleValue::Text("A".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_IS_MENU),
            Some(TrayRoleValue::Bool(true))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_WINDOW_ID),
            Some(TrayRoleValue::Int(42))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_MENU_PATH),
            Some(TrayRoleValue::Text("/MenuBar".to_string()))
        );
        assert_eq!(
            tray_item_role_value(&item, ROLE_HAS_MENU),
            Some(TrayRoleValue::Bool(true))
        );
        assert_eq!(tray_item_role_value(&item, 999), None);

        assert_eq!(tray_role_names().len(), 14);
        for &(r_id, r_name) in tray_role_names() {
            assert!(tray_item_role_value(&item, r_id).is_some());
            assert_ne!(r_name, "");
        }
    }

    #[test]
    fn test_pixmap_cache_operations() {
        let cache = TrayPixmapCache::new();
        assert!(cache.get("non-existent").is_none());

        // Update valid pixmap
        cache.update(
            "item1",
            2,
            2,
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        );
        let entry = cache.get("item1");
        assert!(matches!(entry, Some((2, 2, ref v)) if v.len() == 16));

        // Copy to slice
        let mut w = 0;
        let mut h = 0;
        let mut buf = [0u8; 16];
        let copied = cache.copy_to_slice("item1", &mut w, &mut h, &mut buf);
        assert_eq!(copied, 16);
        assert_eq!(w, 2);
        assert_eq!(h, 2);
        assert_eq!(buf[0], 1);

        // Copy to smaller buffer
        let mut small_buf = [0u8; 4];
        let copied_small = cache.copy_to_slice("item1", &mut w, &mut h, &mut small_buf);
        assert_eq!(copied_small, 4);

        // Copy non-existent
        assert_eq!(cache.copy_to_slice("unknown", &mut w, &mut h, &mut buf), 0);

        // Update invalid/empty clears entry
        cache.update("item1", 0, 0, &[]);
        assert!(cache.get("item1").is_none());

        // Re-add and clear
        cache.update("item2", 1, 1, &[255, 0, 0, 255]);
        assert!(cache.get("item2").is_some());
        cache.clear();
        assert!(cache.get("item2").is_none());

        // Test global instance
        let g = TrayPixmapCache::global();
        g.update("global-test", 1, 1, &[1, 2, 3, 4]);
        assert!(g.get("global-test").is_some());
        g.remove("global-test");
        assert!(g.get("global-test").is_none());

        // Test default
        let d = TrayPixmapCache::default();
        assert!(d.get("anything").is_none());
    }

    #[test]
    fn test_handle_tray_event() {
        let mut state = TrayUiState::new();
        let cache = TrayPixmapCache::new();

        let info = TrayItemInfo {
            id: "app1".to_string(),
            title: "App 1".to_string(),
            status: "Active".to_string(),
            icon_name: "icon1".to_string(),
            icon_theme_path: String::new(),
            icon_pixmap: vec![10, 20, 30, 40],
            icon_pixmap_width: 1,
            icon_pixmap_height: 1,
            tooltip_title: String::new(),
            tooltip_text: String::new(),
            is_menu: false,
            window_id: 0,
            has_attention: false,
            attention_icon_name: String::new(),
            attention_pixmap: Vec::new(),
            attention_pixmap_width: 0,
            attention_pixmap_height: 0,
            menu_path: "/MenuBar".to_string(),
        };

        // ItemAdded
        let modified = handle_tray_event(
            &mut state,
            &cache,
            TrayEvent::ItemAdded(Box::new(info.clone())),
        );
        assert!(modified);
        assert_eq!(state.len(), 1);
        assert!(cache.get("app1").is_some());

        // ItemChanged (empty pixmap)
        let mut info2 = info;
        info2.title = "Updated App 1".to_string();
        info2.icon_pixmap.clear();
        info2.icon_pixmap_width = 0;
        info2.icon_pixmap_height = 0;
        let modified2 =
            handle_tray_event(&mut state, &cache, TrayEvent::ItemChanged(Box::new(info2)));
        assert!(modified2);
        assert_eq!(
            state.get("app1").map(|i| i.title.as_str()),
            Some("Updated App 1")
        );
        assert!(cache.get("app1").is_none());

        // ItemRemoved
        let modified3 = handle_tray_event(
            &mut state,
            &cache,
            TrayEvent::ItemRemoved("app1".to_string()),
        );
        assert!(modified3);
        assert_eq!(state.len(), 0);

        // Remove unknown
        assert!(!handle_tray_event(
            &mut state,
            &cache,
            TrayEvent::ItemRemoved("unknown".to_string())
        ));

        // DaemonDisconnected
        let modified4 = handle_tray_event(&mut state, &cache, TrayEvent::DaemonDisconnected);
        assert!(modified4);
        assert_eq!(state.len(), 0);
    }
}
