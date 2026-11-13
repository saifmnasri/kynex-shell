//! Core unit tests and proptests for kynex-dbusmenu parser, sanitizers, and limits.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::unreadable_literal,
    clippy::doc_markdown,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::redundant_closure_for_method_calls,
    clippy::unnecessary_cast
)]

use kynex_dbusmenu::{
    format_and_sanitize_shortcut, parse_menu_layout, parse_mnemonic_underscores,
    sanitize_icon_name, sanitize_label, strip_markup, validate_menu_event, MenuDisposition,
    MenuError, MenuItemKind, RawMenuNode, RawValue, ToggleKind, ToggleState, MAX_CHILDREN_PER_NODE,
    MAX_LABEL_LEN, MAX_SHORTCUT_LEN, MAX_TOTAL_ITEMS,
};
use proptest::prelude::*;

#[test]
fn test_enum_conversions_and_raw_helpers() {
    assert_eq!(MenuItemKind::Normal.as_u32(), 0);
    assert_eq!(MenuItemKind::Separator.as_u32(), 1);

    assert_eq!(ToggleKind::None.as_u32(), 0);
    assert_eq!(ToggleKind::Checkbox.as_u32(), 1);
    assert_eq!(ToggleKind::Radio.as_u32(), 2);

    assert_eq!(ToggleState::Unknown.as_i32(), -1);
    assert_eq!(ToggleState::Unchecked.as_i32(), 0);
    assert_eq!(ToggleState::Checked.as_i32(), 1);

    assert_eq!(MenuDisposition::Normal.as_u32(), 0);
    assert_eq!(MenuDisposition::Informative.as_u32(), 1);
    assert_eq!(MenuDisposition::Warning.as_u32(), 2);
    assert_eq!(MenuDisposition::Alert.as_u32(), 3);

    let map_val = RawValue::Map(vec![("k".to_string(), RawValue::Bool(true))]);
    assert_eq!(map_val.as_map().map(|m| m.len()), Some(1));
    assert_eq!(RawValue::Int(42).as_map(), None);
    assert_eq!(
        RawValue::Bytes(vec![1, 2, 3]),
        RawValue::Bytes(vec![1, 2, 3])
    );
}

#[test]
fn test_strip_markup() {
    assert_eq!(strip_markup("<b>Hello</b> <i>World</i>"), "Hello World");
    assert_eq!(
        strip_markup("<script>evil()</script>Normal"),
        "evil()Normal"
    );
    assert_eq!(strip_markup("No markup"), "No markup");
    assert_eq!(strip_markup("<tag with=\"attr\">Text</tag>"), "Text");
}

#[test]
fn test_parse_mnemonic_underscores() {
    assert_eq!(parse_mnemonic_underscores("_File"), "File");
    assert_eq!(parse_mnemonic_underscores("__"), "_");
    assert_eq!(parse_mnemonic_underscores("Save _As..."), "Save As...");
    assert_eq!(parse_mnemonic_underscores("foo__bar"), "foo_bar");
    assert_eq!(parse_mnemonic_underscores("_"), "");
    assert_eq!(parse_mnemonic_underscores("___"), "_");
    assert_eq!(parse_mnemonic_underscores("____"), "__");
    assert_eq!(parse_mnemonic_underscores("E_xit"), "Exit");
    assert_eq!(parse_mnemonic_underscores("A_B_C"), "ABC");
}

#[test]
fn test_sanitize_label() {
    // Basic label
    assert_eq!(sanitize_label("_File"), "File");

    // Markup + mnemonic
    assert_eq!(sanitize_label("<b>_Open</b>..."), "Open...");

    // Control characters dropped
    assert_eq!(sanitize_label("Line1\nLine2\x07\x1B[31m"), "Line1Line2[31m");

    // Unicode bidi overrides dropped
    let bidi_str = "Status: \u{202E}gnirtS\u{202C}";
    assert_eq!(sanitize_label(bidi_str), "Status: gnirtS");

    // Long string clamped to 256 chars
    let huge = "a".repeat(10_000);
    let sanitized = sanitize_label(&huge);
    assert_eq!(sanitized.chars().count(), MAX_LABEL_LEN);
}

#[test]
fn test_sanitize_icon_name() {
    assert_eq!(sanitize_icon_name("document-save"), "document-save");
    assert_eq!(
        sanitize_icon_name("edit-undo.symbolic"),
        "edit-undo.symbolic"
    );
    assert_eq!(sanitize_icon_name("icon_42"), "icon_42");

    // Prohibited raw paths, slashes, traversal
    assert_eq!(sanitize_icon_name("/usr/share/icons/evil.png"), "");
    assert_eq!(sanitize_icon_name("../../etc/passwd"), "");
    assert_eq!(sanitize_icon_name("path\\windows"), "");

    // Invalid characters
    assert_eq!(sanitize_icon_name("icon with spaces"), "");
    assert_eq!(sanitize_icon_name("icon$name"), "");

    // Excess length
    let long_name = "a".repeat(129);
    assert_eq!(sanitize_icon_name(&long_name), "");
}

#[test]
fn test_format_and_sanitize_shortcut() {
    // String variant
    let s_val = RawValue::String("Ctrl+S".to_string());
    assert_eq!(format_and_sanitize_shortcut(&s_val), "Ctrl+S");

    // Array of tokens
    let arr_val = RawValue::Array(vec![
        RawValue::String("Control".to_string()),
        RawValue::String("Shift".to_string()),
        RawValue::String("s".to_string()),
    ]);
    assert_eq!(format_and_sanitize_shortcut(&arr_val), "Ctrl+Shift+S");

    // Nested array (array of arrays)
    let nested_val = RawValue::Array(vec![RawValue::Array(vec![
        RawValue::String("Alt".to_string()),
        RawValue::String("F4".to_string()),
    ])]);
    assert_eq!(format_and_sanitize_shortcut(&nested_val), "Alt+F4");

    // Huge shortcut clamped to MAX_SHORTCUT_LEN
    let huge_val = RawValue::String("A+".repeat(100));
    let formatted = format_and_sanitize_shortcut(&huge_val);
    assert_eq!(formatted.chars().count(), MAX_SHORTCUT_LEN);
}

#[test]
fn test_normal_layout_parsing() {
    let mut root = RawMenuNode::new(0);

    let child1 = RawMenuNode::new(1)
        .with_property("label", RawValue::String("_File".to_string()))
        .with_property("enabled", RawValue::Bool(true))
        .with_property("visible", RawValue::Bool(true))
        .with_child(
            RawMenuNode::new(10)
                .with_property("label", RawValue::String("_New".to_string()))
                .with_property("shortcut", RawValue::String("Ctrl+N".to_string())),
        )
        .with_child(
            RawMenuNode::new(11)
                .with_property("label", RawValue::String("_Open".to_string()))
                .with_property("toggle-type", RawValue::String("checkmark".to_string()))
                .with_property("toggle-state", RawValue::Int(1)),
        );

    let sep = RawMenuNode::new(2).with_property("type", RawValue::String("separator".to_string()));

    let child3 = RawMenuNode::new(3)
        .with_property("label", RawValue::String("E_xit".to_string()))
        .with_property("disposition", RawValue::String("alert".to_string()));

    root.children.push(child1);
    root.children.push(sep);
    root.children.push(child3);

    let layout = parse_menu_layout(42, &root).expect("valid layout must parse");
    assert_eq!(layout.revision, 42);
    assert_eq!(layout.items.len(), 5);

    // Child 1
    assert_eq!(layout.items[0].id, 1);
    assert_eq!(layout.items[0].parent, 0);
    assert_eq!(layout.items[0].label, "File");
    assert!(layout.items[0].has_submenu);

    // Subchild 10
    assert_eq!(layout.items[1].id, 10);
    assert_eq!(layout.items[1].parent, 1);
    assert_eq!(layout.items[1].label, "New");
    assert_eq!(layout.items[1].shortcut, "Ctrl+N");
    assert!(!layout.items[1].has_submenu);

    // Subchild 11 (checkbox checked)
    assert_eq!(layout.items[2].id, 11);
    assert_eq!(layout.items[2].parent, 1);
    assert_eq!(layout.items[2].toggle, ToggleKind::Checkbox);
    assert_eq!(layout.items[2].toggle_state, ToggleState::Checked);

    // Separator
    assert_eq!(layout.items[3].id, 2);
    assert_eq!(layout.items[3].kind, MenuItemKind::Separator);

    // Exit (disposition Alert)
    assert_eq!(layout.items[4].id, 3);
    assert_eq!(layout.items[4].label, "Exit");
    assert_eq!(layout.items[4].disposition, MenuDisposition::Alert);
}

#[test]
fn test_depth_cap_enforcement() {
    // Build tree of exact depth 6 (root is 0, children down to depth 6)
    let mut current = RawMenuNode::new(6);
    for d in (1..=5).rev() {
        let mut parent = RawMenuNode::new(d as i32);
        parent.children.push(current);
        current = parent;
    }
    let mut root = RawMenuNode::new(0);
    root.children.push(current);

    assert!(parse_menu_layout(1, &root).is_ok(), "depth 6 must succeed");

    // Build tree of depth 7
    let mut current = RawMenuNode::new(7);
    for d in (1..=6).rev() {
        let mut parent = RawMenuNode::new(d as i32);
        parent.children.push(current);
        current = parent;
    }
    let mut root = RawMenuNode::new(0);
    root.children.push(current);

    let err = parse_menu_layout(1, &root).expect_err("depth 7 must be rejected");
    assert_eq!(err, MenuError::DepthExceeded(7));

    // Hostile 1,000 levels
    let mut current = RawMenuNode::new(1000);
    for d in (1..=999).rev() {
        let mut parent = RawMenuNode::new(d as i32);
        parent.children.push(current);
        current = parent;
    }
    let mut root = RawMenuNode::new(0);
    root.children.push(current);
    assert!(matches!(
        parse_menu_layout(1, &root),
        Err(MenuError::DepthExceeded(_))
    ));
}

#[test]
fn test_total_items_cap_enforcement() {
    // 500 items across 5 nodes of 100 children
    let mut root = RawMenuNode::new(0);
    let mut id_counter = 1;
    for _ in 0..5 {
        let mut parent = RawMenuNode::new(id_counter);
        id_counter += 1;
        for _ in 0..99 {
            parent.children.push(RawMenuNode::new(id_counter));
            id_counter += 1;
        }
        root.children.push(parent);
    }
    assert_eq!(id_counter - 1, MAX_TOTAL_ITEMS as i32);
    assert!(
        parse_menu_layout(1, &root).is_ok(),
        "500 items must succeed"
    );

    // Add 1 more item (501 items)
    root.children[0].children.push(RawMenuNode::new(9999));
    let err = parse_menu_layout(1, &root).expect_err("501 items must be rejected");
    assert_eq!(err, MenuError::ItemCountExceeded(501));
}

#[test]
fn test_child_count_cap_enforcement() {
    let mut root = RawMenuNode::new(0);
    // Add 100 children to root
    for id in 1..=MAX_CHILDREN_PER_NODE {
        root.children.push(RawMenuNode::new(id as i32));
    }
    assert!(
        parse_menu_layout(1, &root).is_ok(),
        "100 children must succeed"
    );

    // Add 101st child
    root.children.push(RawMenuNode::new(101));
    let err = parse_menu_layout(1, &root).expect_err("101 children must be rejected");
    assert_eq!(err, MenuError::ChildCountExceeded(101));
}

#[test]
fn test_duplicate_and_negative_id_rejection() {
    // Duplicate IDs
    let mut root = RawMenuNode::new(0);
    root.children.push(RawMenuNode::new(1));
    root.children.push(RawMenuNode::new(1)); // Duplicate!
    assert_eq!(parse_menu_layout(1, &root), Err(MenuError::DuplicateId(1)));

    // Negative ID
    let mut root = RawMenuNode::new(0);
    root.children.push(RawMenuNode::new(-5));
    assert_eq!(parse_menu_layout(1, &root), Err(MenuError::NegativeId(-5)));
}

#[test]
fn test_cycle_and_self_ancestor_rejection() {
    // Node is its own child (self-ancestor cycle)
    let mut root = RawMenuNode::new(0);
    let mut node = RawMenuNode::new(1);
    node.children.push(RawMenuNode::new(1)); // Self-ancestor!
    root.children.push(node);
    assert_eq!(parse_menu_layout(1, &root), Err(MenuError::SelfAncestor(1)));
}

#[test]
fn test_property_type_confusion_tolerance() {
    let mut root = RawMenuNode::new(0);
    let item = RawMenuNode::new(1)
        // Mismatched types
        .with_property("label", RawValue::Int(12345))
        .with_property("enabled", RawValue::String("true".to_string()))
        .with_property("visible", RawValue::Array(vec![]))
        .with_property("toggle-state", RawValue::String("checked".to_string()))
        .with_property("disposition", RawValue::Bool(true));
    root.children.push(item);

    let layout = parse_menu_layout(1, &root).expect("type mismatch must not crash");
    assert_eq!(layout.items.len(), 1);
    assert_eq!(layout.items[0].label, "");
    assert!(layout.items[0].enabled);
    assert!(layout.items[0].visible);
    assert_eq!(layout.items[0].toggle_state, ToggleState::Unchecked);
    assert_eq!(layout.items[0].disposition, MenuDisposition::Normal);
}

#[test]
fn test_invisible_items_dropped_and_disabled_kept() {
    let mut root = RawMenuNode::new(0);

    // Visible enabled item
    root.children
        .push(RawMenuNode::new(1).with_property("label", RawValue::String("Item 1".to_string())));

    // Disabled item
    root.children.push(
        RawMenuNode::new(2)
            .with_property("label", RawValue::String("Disabled".to_string()))
            .with_property("enabled", RawValue::Bool(false)),
    );

    // Invisible item with child
    root.children.push(
        RawMenuNode::new(3)
            .with_property("label", RawValue::String("Hidden".to_string()))
            .with_property("visible", RawValue::Bool(false))
            .with_child(
                RawMenuNode::new(4)
                    .with_property("label", RawValue::String("Sub-hidden".to_string())),
            ),
    );

    let layout = parse_menu_layout(1, &root).expect("layout should parse");
    assert_eq!(layout.items.len(), 2);
    assert_eq!(layout.items[0].id, 1);
    assert!(layout.items[0].enabled);

    assert_eq!(layout.items[1].id, 2);
    assert!(!layout.items[1].enabled); // Kept but disabled
}

#[test]
fn test_event_validation() {
    let mut root = RawMenuNode::new(0);
    root.children.push(
        RawMenuNode::new(1)
            .with_property("label", RawValue::String("Enabled".to_string()))
            .with_property("enabled", RawValue::Bool(true)),
    );
    root.children.push(
        RawMenuNode::new(2)
            .with_property("label", RawValue::String("Disabled".to_string()))
            .with_property("enabled", RawValue::Bool(false)),
    );
    root.children
        .push(RawMenuNode::new(3).with_property("type", RawValue::String("separator".to_string())));

    let layout = parse_menu_layout(1, &root).expect("layout should parse");

    // Valid events on enabled item
    assert!(validate_menu_event(&layout, 1, "clicked").is_ok());
    assert!(validate_menu_event(&layout, 1, "hovered").is_ok());

    // Invalid event kinds
    assert_eq!(
        validate_menu_event(&layout, 1, "double-click"),
        Err(MenuError::InvalidEventKind("double-click".to_string()))
    );
    assert_eq!(
        validate_menu_event(&layout, 1, "activated"),
        Err(MenuError::InvalidEventKind("activated".to_string()))
    );

    // Disabled item
    assert_eq!(
        validate_menu_event(&layout, 2, "clicked"),
        Err(MenuError::ItemDisabled(2))
    );

    // Separator item
    assert_eq!(
        validate_menu_event(&layout, 3, "clicked"),
        Err(MenuError::ItemIsSeparator(3))
    );

    // Unknown ID
    assert_eq!(
        validate_menu_event(&layout, 999, "clicked"),
        Err(MenuError::ItemNotFound(999))
    );
}

// Proptest: ensure parse_menu_layout never panics on arbitrary raw structures
proptest! {
    #[test]
    fn proptest_arbitrary_layout_no_panic(
        rev in any::<u32>(),
        root_id in any::<i32>(),
        num_children in 0usize..20,
    ) {
        let mut root = RawMenuNode::new(root_id);
        for i in 0..num_children {
            let child = RawMenuNode::new((i + 1) as i32);
            root.children.push(child);
        }
        // Result is either Ok or Err, but never panics
        let _ = parse_menu_layout(rev, &root);
    }
}
