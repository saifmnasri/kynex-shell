//! Menu event validation ensuring only authorized actions reach external processes.

use crate::error::MenuError;
use crate::types::{MenuItemKind, ValidatedMenuLayout};

/// Validates that an incoming menu event is permitted under the current layout.
///
/// Invariants enforced:
/// 1. Target ID must exist in `layout.items`.
/// 2. Target item must NOT be a separator (`kind != MenuItemKind::Separator`).
/// 3. Target item must be enabled (`enabled == true`).
/// 4. Event kind must be strictly `"clicked"` or `"hovered"`.
///
/// # Errors
/// Returns [`MenuError::InvalidEventKind`] if event name is unknown.
/// Returns [`MenuError::ItemNotFound`] if the ID does not exist in `layout`.
/// Returns [`MenuError::ItemIsSeparator`] if the item is a separator.
/// Returns [`MenuError::ItemDisabled`] if the item is disabled.
pub fn validate_menu_event(
    layout: &ValidatedMenuLayout,
    item_id: i32,
    event: &str,
) -> Result<(), MenuError> {
    if event != "clicked" && event != "hovered" {
        return Err(MenuError::InvalidEventKind(event.to_string()));
    }

    let item = layout
        .find_item(item_id)
        .ok_or(MenuError::ItemNotFound(item_id))?;

    if item.kind == MenuItemKind::Separator {
        return Err(MenuError::ItemIsSeparator(item_id));
    }

    if !item.enabled {
        return Err(MenuError::ItemDisabled(item_id));
    }

    Ok(())
}
