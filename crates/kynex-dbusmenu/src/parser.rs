//! Recursive validator and parser transforming untrusted menu trees into flat, validated structures.

use std::collections::{HashMap, HashSet};

use crate::error::MenuError;
use crate::limits::{MAX_CHILDREN_PER_NODE, MAX_MENU_DEPTH, MAX_TOTAL_ITEMS};
use crate::raw::{RawMenuNode, RawValue};
use crate::sanitize::{format_and_sanitize_shortcut, sanitize_icon_name, sanitize_label};
use crate::types::{
    MenuDisposition, MenuItemKind, ToggleKind, ToggleState, ValidatedMenuItem, ValidatedMenuLayout,
};

/// Traverses and validates the full raw menu tree, enforcing structural bounds,
/// cycle detection, ID uniqueness, and resource limits.
fn validate_tree_structure(
    node: &RawMenuNode,
    current_depth: usize,
    active_path: &mut Vec<i32>,
    seen_ids: &mut HashSet<i32>,
    total_count: &mut usize,
    is_root: bool,
) -> Result<(), MenuError> {
    if current_depth > MAX_MENU_DEPTH {
        return Err(MenuError::DepthExceeded(current_depth));
    }

    if node.children.len() > MAX_CHILDREN_PER_NODE {
        return Err(MenuError::ChildCountExceeded(node.children.len()));
    }

    // Negative ID check (only root is permitted to have ID 0 or positive; negative IDs rejected)
    if node.id < 0 {
        return Err(MenuError::NegativeId(node.id));
    }

    // Cycle detection along the current ancestor path
    if active_path.contains(&node.id) {
        if let Some(&ancestor) = active_path.last() {
            if ancestor == node.id {
                return Err(MenuError::SelfAncestor(node.id));
            }
        }
        return Err(MenuError::CycleDetected(node.id));
    }

    // Uniqueness across the entire tree
    if seen_ids.contains(&node.id) {
        return Err(MenuError::DuplicateId(node.id));
    }
    seen_ids.insert(node.id);

    // Count item (root node is a container and not counted in item quota if depth == 0)
    if !is_root {
        *total_count += 1;
        if *total_count > MAX_TOTAL_ITEMS {
            return Err(MenuError::ItemCountExceeded(*total_count));
        }
    }

    active_path.push(node.id);

    for child in &node.children {
        validate_tree_structure(
            child,
            current_depth + 1,
            active_path,
            seen_ids,
            total_count,
            false,
        )?;
    }

    active_path.pop();
    Ok(())
}

/// Extracts strongly typed and sanitized property values for a single menu item.
fn parse_item_properties(
    props: &HashMap<String, RawValue>,
) -> (
    String,
    bool,
    bool,
    MenuItemKind,
    ToggleKind,
    ToggleState,
    String,
    MenuDisposition,
    String,
) {
    let label = props
        .get("label")
        .and_then(RawValue::as_str)
        .map(sanitize_label)
        .unwrap_or_default();

    let enabled = props
        .get("enabled")
        .and_then(RawValue::as_bool)
        .unwrap_or(true);

    let visible = props
        .get("visible")
        .and_then(RawValue::as_bool)
        .unwrap_or(true);

    let kind = match props.get("type").and_then(RawValue::as_str) {
        Some("separator") => MenuItemKind::Separator,
        _ => MenuItemKind::Normal,
    };

    let toggle = match props.get("toggle-type").and_then(RawValue::as_str) {
        Some("checkmark") => ToggleKind::Checkbox,
        Some("radio") => ToggleKind::Radio,
        _ => ToggleKind::None,
    };

    let toggle_state = match toggle {
        ToggleKind::None => ToggleState::Unchecked,
        ToggleKind::Checkbox | ToggleKind::Radio => {
            match props.get("toggle-state").and_then(RawValue::as_i64) {
                Some(1) => ToggleState::Checked,
                Some(0) | None => ToggleState::Unchecked,
                Some(_) => ToggleState::Unknown,
            }
        }
    };

    let icon_name = props
        .get("icon-name")
        .and_then(RawValue::as_str)
        .map(sanitize_icon_name)
        .unwrap_or_default();

    let disposition = match props.get("disposition").and_then(RawValue::as_str) {
        Some("informative") => MenuDisposition::Informative,
        Some("warning") => MenuDisposition::Warning,
        Some("alert") => MenuDisposition::Alert,
        _ => MenuDisposition::Normal,
    };

    let shortcut = props
        .get("shortcut")
        .map(format_and_sanitize_shortcut)
        .unwrap_or_default();

    (
        label,
        enabled,
        visible,
        kind,
        toggle,
        toggle_state,
        icon_name,
        disposition,
        shortcut,
    )
}

/// Flattens visible menu items in pre-order traversal into the destination list.
fn collect_visible_items(node: &RawMenuNode, parent_id: i32, out: &mut Vec<ValidatedMenuItem>) {
    for child in &node.children {
        let (label, enabled, visible, kind, toggle, toggle_state, icon_name, disposition, shortcut) =
            parse_item_properties(&child.properties);

        // Invisible items and their children are dropped
        if !visible {
            continue;
        }

        // Check if child has at least one visible sub-item
        let has_submenu = child.children.iter().any(|c| {
            c.properties
                .get("visible")
                .and_then(RawValue::as_bool)
                .unwrap_or(true)
        });

        out.push(ValidatedMenuItem {
            id: child.id,
            parent: parent_id,
            label,
            enabled,
            visible,
            kind,
            toggle,
            toggle_state,
            icon_name,
            disposition,
            shortcut,
            has_submenu,
        });

        if has_submenu {
            collect_visible_items(child, child.id, out);
        }
    }
}

/// Parses and validates a raw dbusmenu layout tree into a [`ValidatedMenuLayout`].
///
/// Enforces all security and resource bounds:
/// - Depth <= 6
/// - Total items <= 500
/// - Children per node <= 100
/// - Unique non-negative IDs, cycle and self-ancestor rejection
/// - Property type validation with safe fallbacks
/// - Dropping invisible items
/// - Mnemonic underscore and string sanitization
///
/// # Errors
/// Returns [`MenuError`] if any structural bound is violated.
pub fn parse_menu_layout(
    revision: u32,
    root: &RawMenuNode,
) -> Result<ValidatedMenuLayout, MenuError> {
    let mut active_path = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut total_count = 0;

    validate_tree_structure(
        root,
        0,
        &mut active_path,
        &mut seen_ids,
        &mut total_count,
        true,
    )?;

    let mut items = Vec::with_capacity(total_count);
    collect_visible_items(root, root.id, &mut items);

    Ok(ValidatedMenuLayout { revision, items })
}
