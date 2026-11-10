//! Property-based tests for StatusNotifierItem pixmap validator and string sanitizer.
#![allow(clippy::doc_markdown, clippy::cast_sign_loss)]

use kynex_tray::limits::MAX_PIXMAP_DIMENSION;
use kynex_tray::pixmap::{select_best_pixmap, validate_and_convert_pixmap};
use kynex_tray::sanitize::{
    sanitize_icon_name, sanitize_id, sanitize_text, sanitize_title, sanitize_tooltip_text,
    sanitize_tooltip_title,
};
use proptest::prelude::*;

proptest! {
    /// Sanitizer must never crash, never produce forbidden controls or bidi overrides,
    /// and never exceed max requested length.
    #[test]
    fn proptest_sanitize_text(s in "\\PC*", max_chars in 0usize..256) {
        let cleaned = sanitize_text(&s, max_chars);
        prop_assert!(cleaned.chars().count() <= max_chars);

        for ch in cleaned.chars() {
            let u = ch as u32;
            let is_bidi = matches!(
                ch,
                '\u{202A}'..='\u{202E}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{200E}'
                    | '\u{200F}'
                    | '\u{061C}'
            );
            prop_assert!(!is_bidi);
            if u != 0x0A && u != 0x09 {
                let forbidden = u <= 0x1F || u == 0x7F || (0x80..=0x9F).contains(&u);
                prop_assert!(!forbidden);
            }
        }
    }

    #[test]
    fn proptest_sanitize_helpers(s in "\\PC*") {
        let _ = sanitize_id(&s);
        let _ = sanitize_title(&s);
        let _ = sanitize_tooltip_title(&s);
        let _ = sanitize_tooltip_text(&s);
        let _ = sanitize_icon_name(&s);
    }

    /// Pixmap validator must never panic on arbitrary width, height, and data buffers.
    #[test]
    fn proptest_pixmap_validator(w in -100i32..200i32, h in -100i32..200i32, data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let res = validate_and_convert_pixmap(w, h, &data);
        if let Ok(valid) = res {
            prop_assert!(valid.width > 0 && valid.width <= MAX_PIXMAP_DIMENSION);
            prop_assert!(valid.height > 0 && valid.height <= MAX_PIXMAP_DIMENSION);
            let expected_len = (valid.width as usize) * (valid.height as usize) * 4;
            prop_assert_eq!(valid.rgba_data.len(), expected_len);
        }
    }

    /// Pixmap selector must never panic on arbitrary lists of raw tuples.
    #[test]
    fn proptest_select_best_pixmap(list in prop::collection::vec((any::<i32>(), any::<i32>(), prop::collection::vec(any::<u8>(), 0..256)), 0..10)) {
        let _ = select_best_pixmap(&list);
    }
}
