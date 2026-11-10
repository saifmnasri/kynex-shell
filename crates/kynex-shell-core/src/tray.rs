//! Pure-Rust state machine and data model logic for the system tray in `kynex-shell`.

use kynex_ipc::tray::TrayItemInfo;

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
        };

        let ui_item = TrayUiItem::from_ipc(&info);
        assert_eq!(ui_item.id, ":1.42/StatusNotifierItem");
        assert_eq!(ui_item.title, "Test App");
        assert!(ui_item.has_pixmap);
        assert!(ui_item.is_attention);
        assert_eq!(ui_item.letter_badge, "T");
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
            icon_pixmap: Vec::new(),
            icon_pixmap_width: 0,
            icon_pixmap_height: 0,
        };

        let (idx, is_update) = state.upsert(item1.clone());
        assert_eq!(idx, 0);
        assert!(!is_update);
        assert_eq!(state.len(), 1);

        // Update item 1
        let mut updated = item1;
        updated.title = "Updated Item 1".to_string();
        let (idx_u, is_u) = state.upsert(updated);
        assert_eq!(idx_u, 0);
        assert!(is_u);
        assert_eq!(
            state.get("item1").map(|it| it.title.as_str()),
            Some("Updated Item 1")
        );

        // Remove item 1
        let rem = state.remove("item1");
        assert!(rem.is_some());
        assert_eq!(state.len(), 0);
        assert!(state.remove("item1").is_none());
    }
}
