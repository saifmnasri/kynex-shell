//! Property-based testing for sanitizer and image data validator invariants.

use kynex_notifications::{
    sanitize_text, truncate_to_char_boundary, validate_image_data, MAX_IMAGE_BYTES,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_sanitize_text_never_panics_and_respects_invariants(
        input in ".*",
        max_bytes in 0usize..8192
    ) {
        let result = sanitize_text(&input, max_bytes);

        // Invariant 1: Result length never exceeds max_bytes
        prop_assert!(result.len() <= max_bytes);

        // Invariant 2: Result is always valid UTF-8 sliced on a char boundary
        prop_assert!(std::str::from_utf8(result.as_bytes()).is_ok());

        // Invariant 3: Result contains no forbidden ASCII control characters (C0 / DEL)
        for ch in result.chars() {
            let u = ch as u32;
            if u != 0x0A && u != 0x09 {
                prop_assert!(u > 0x1F, "Found control char 0x{:02X}", u);
                prop_assert_ne!(u, 0x7F, "Found DEL char");
                prop_assert!(!(0x80..=0x9F).contains(&u), "Found C1 control char 0x{:02X}", u);
            }
        }

        // Invariant 4: Result contains no Unicode bidi overrides or isolates
        for ch in result.chars() {
            prop_assert!(
                !matches!(
                    ch,
                    '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
                ),
                "Found bidi override character: U+{:04X}",
                ch as u32
            );
        }
    }

    #[test]
    fn prop_truncate_to_char_boundary_always_safe(
        input in ".*",
        max_bytes in 0usize..8192
    ) {
        let truncated = truncate_to_char_boundary(&input, max_bytes);
        prop_assert!(truncated.len() <= max_bytes);
        prop_assert!(input.starts_with(truncated));
    }

    #[test]
    fn prop_validate_image_data_never_panics(
        width in any::<i32>(),
        height in any::<i32>(),
        rowstride in any::<i32>(),
        has_alpha in any::<bool>(),
        bits_per_sample in any::<i32>(),
        channels in any::<i32>(),
        data in prop::collection::vec(any::<u8>(), 0..1024)
    ) {
        // Must never panic on arbitrary inputs (including negative, overflow products, lying headers)
        let outcome = validate_image_data(
            width,
            height,
            rowstride,
            has_alpha,
            bits_per_sample,
            channels,
            &data,
        );

        if let Ok(valid) = outcome {
            prop_assert!(valid.width > 0 && valid.width <= 256);
            prop_assert!(valid.height > 0 && valid.height <= 256);
            prop_assert!(valid.channels == 3 || valid.channels == 4);
            prop_assert_eq!(valid.has_alpha, valid.channels == 4);
            prop_assert_eq!(valid.bits_per_sample, 8);
            prop_assert!(valid.data.len() <= MAX_IMAGE_BYTES);
            prop_assert!(valid.rowstride >= valid.width * valid.channels);
        }
    }

    #[test]
    fn prop_validate_action_key_invariants(key in ".*") {
        let res = kynex_notifications::validate_action_key(&key);
        if res.is_ok() {
            prop_assert!(!key.is_empty());
            prop_assert!(key.len() <= kynex_notifications::MAX_ACTION_KEY_LEN);
            prop_assert!(key.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'));
        }
    }

    #[test]
    fn prop_markup_stripper_invariants(input in ".*") {
        let stripped = kynex_notifications::sanitize_body(&input);
        prop_assert!(stripped.len() <= kynex_notifications::MAX_BODY_BYTES);
        prop_assert!(std::str::from_utf8(stripped.as_bytes()).is_ok());
    }
}
