//! Property-based tests for `kynex-dbusmenu` layout parser and validator.
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
    parse_menu_layout, sanitize_label, validate_menu_event, MenuItemKind, RawMenuNode, RawValue,
    MAX_LABEL_LEN, MAX_TOTAL_ITEMS,
};
use proptest::collection::vec;
use proptest::prelude::*;
use std::collections::HashMap;

fn arb_raw_value() -> impl Strategy<Value = RawValue> {
    prop_oneof![
        any::<String>().prop_map(RawValue::String),
        any::<bool>().prop_map(RawValue::Bool),
        any::<i64>().prop_map(RawValue::Int),
        vec(any::<String>(), 0..5)
            .prop_map(|v| { RawValue::Array(v.into_iter().map(RawValue::String).collect()) }),
    ]
}

fn arb_raw_node(max_depth: u32) -> impl Strategy<Value = RawMenuNode> {
    let leaf =
        (any::<i32>(), vec((any::<String>(), arb_raw_value()), 0..8)).prop_map(|(id, props)| {
            let mut map = HashMap::new();
            for (k, v) in props {
                map.insert(k, v);
            }
            RawMenuNode {
                id,
                properties: map,
                children: Vec::new(),
            }
        });

    leaf.prop_recursive(
        max_depth,
        128, // max items total
        16,  // max children per branch
        |inner| {
            (
                any::<i32>(),
                vec((any::<String>(), arb_raw_value()), 0..8),
                vec(inner, 0..8),
            )
                .prop_map(|(id, props, children)| {
                    let mut map = HashMap::new();
                    for (k, v) in props {
                        map.insert(k, v);
                    }
                    RawMenuNode {
                        id,
                        properties: map,
                        children,
                    }
                })
        },
    )
}

fn is_forbidden_char(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    ) || (c.is_ascii_control() && c != ' ')
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_label_sanitizer_invariants(s in ".*") {
        let sanitized = sanitize_label(&s);

        // Invariant 1: length bounded
        prop_assert!(sanitized.chars().count() <= MAX_LABEL_LEN);

        // Invariant 2: no forbidden Unicode bidi overrides or isolates or ASCII control chars
        for c in sanitized.chars() {
            prop_assert!(!is_forbidden_char(c));
        }
    }

    #[test]
    fn prop_layout_parser_invariants(root in arb_raw_node(8)) {
        let res = parse_menu_layout(1, &root);
        if let Ok(layout) = res {
            // Invariant 1: total items <= MAX_TOTAL_ITEMS
            prop_assert!(layout.items.len() <= MAX_TOTAL_ITEMS);

            // Invariant 2: unique non-negative IDs
            let mut ids = std::collections::HashSet::new();
            for item in &layout.items {
                prop_assert!(item.id >= 0);
                prop_assert!(ids.insert(item.id));
                prop_assert!(item.label.chars().count() <= MAX_LABEL_LEN);

                if item.kind == MenuItemKind::Separator {
                    prop_assert!(!item.has_submenu);
                    prop_assert_eq!(item.toggle, kynex_dbusmenu::ToggleKind::None);
                }
            }
        }
    }

    #[test]
    fn prop_event_validation_invariants(
        root in arb_raw_node(4),
        event_id in any::<i32>(),
        event_name in any::<String>()
    ) {
        if let Ok(layout) = parse_menu_layout(1, &root) {
            let res = validate_menu_event(&layout, event_id, &event_name);
            if res.is_ok() {
                // Must be "clicked" or "hovered"
                prop_assert!(event_name == "clicked" || event_name == "hovered");
                // Must exist in layout
                let item = layout.items.iter().find(|i| i.id == event_id);
                prop_assert!(item.is_some());
                let it = item.unwrap();
                prop_assert!(it.enabled);
                prop_assert_ne!(it.kind, MenuItemKind::Separator);
            }
        }
    }
}
