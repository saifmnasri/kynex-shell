#![no_main]

use libfuzzer_sys::fuzz_target;
use kynex_dbusmenu::{
    parse_menu_layout, validate_menu_event, RawMenuNode, RawValue,
    MAX_LABEL_LEN, MAX_SHORTCUT_LEN, MAX_TOTAL_ITEMS,
};

fn build_node_from_bytes(data: &[u8], offset: &mut usize, depth: usize) -> Option<RawMenuNode> {
    if *offset >= data.len() || depth > 10 {
        return None;
    }

    let b = data[*offset];
    *offset += 1;

    let id = (b as i32) - 10; // Can be negative, 0, or positive
    let mut node = RawMenuNode::new(id);

    // Optional property injection based on remaining bytes
    if *offset < data.len() {
        let prop_type = data[*offset] % 8;
        *offset += 1;

        let end = (*offset + 32).min(data.len());
        let slice = &data[*offset..end];
        *offset = end;
        let text = String::from_utf8_lossy(slice).to_string();

        match prop_type {
            0 => {
                node.properties.insert("label".to_string(), RawValue::String(text));
            }
            1 => {
                node.properties.insert("enabled".to_string(), RawValue::Bool(b % 2 == 0));
            }
            2 => {
                node.properties.insert("visible".to_string(), RawValue::Bool(b % 2 == 0));
            }
            3 => {
                node.properties.insert("type".to_string(), RawValue::String(if b % 2 == 0 { "separator".to_string() } else { "normal".to_string() }));
            }
            4 => {
                node.properties.insert("toggle-type".to_string(), RawValue::String(text));
            }
            5 => {
                node.properties.insert("toggle-state".to_string(), RawValue::Int((b as i64) - 2));
            }
            6 => {
                node.properties.insert("disposition".to_string(), RawValue::String(text));
            }
            7 => {
                node.properties.insert("shortcut".to_string(), RawValue::String(text));
            }
            _ => {}
        }
    }

    // Number of children to attempt (bounded to prevent huge loops in fuzzer generator)
    if *offset < data.len() {
        let child_count = (data[*offset] % 8) as usize;
        *offset += 1;
        for _ in 0..child_count {
            if let Some(child) = build_node_from_bytes(data, offset, depth + 1) {
                node.children.push(child);
            }
        }
    }

    Some(node)
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let mut offset = 0;
    let root = match build_node_from_bytes(data, &mut offset, 0) {
        Some(r) => r,
        None => return,
    };

    if let Ok(layout) = parse_menu_layout(1, &root) {
        assert!(layout.items.len() <= MAX_TOTAL_ITEMS);
        for item in &layout.items {
            assert!(item.label.chars().count() <= MAX_LABEL_LEN);
            assert!(item.shortcut.chars().count() <= MAX_SHORTCUT_LEN);
            assert!(item.id >= 0);

            // Test event validation on valid layout
            let _ = validate_menu_event(&layout, item.id, "clicked");
            let _ = validate_menu_event(&layout, item.id, "hovered");
            let _ = validate_menu_event(&layout, item.id, "invalid");
        }
    }
});
