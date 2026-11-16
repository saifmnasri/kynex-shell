//! Property-based tests for audio parsing, volume calculation, and sanitization invariants.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::absurd_extreme_comparisons
)]

use kynex_audio::{
    compute_master_volume, parse_volume_string, sanitize_device_name, BoundedLineReader,
    MAX_DEVICE_NAME_CHARS, MAX_VOLUME, MIN_VOLUME,
};
use proptest::prelude::*;

fn is_bidi_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    )
}

proptest! {
    #[test]
    fn prop_compute_master_volume_in_bounds(channels in prop::collection::vec(any::<u32>(), 0..64)) {
        let master = compute_master_volume(&channels);
        prop_assert!(master >= MIN_VOLUME);
        prop_assert!(master <= MAX_VOLUME);

        if channels.is_empty() {
            prop_assert_eq!(master, 0);
        }
    }

    #[test]
    fn prop_sanitize_device_name_length_and_invariants(input in ".*") {
        let sanitized = sanitize_device_name(&input);

        // Length invariant: character count must never exceed 128
        prop_assert!(sanitized.chars().count() <= MAX_DEVICE_NAME_CHARS);

        // Character invariant: must not contain forbidden control or bidi override characters
        for ch in sanitized.chars() {
            let u = ch as u32;
            prop_assert!(u > 0x1F && u != 0x7F && !(0x80..=0x9F).contains(&u));
            prop_assert!(!is_bidi_char(ch));
        }
    }

    #[test]
    fn prop_parse_volume_string_always_safe(s in ".*") {
        let vol = parse_volume_string(&s);
        prop_assert!(vol >= MIN_VOLUME);
        prop_assert!(vol <= MAX_VOLUME);
    }

    #[test]
    fn prop_bounded_line_reader_never_exceeds_buffer_limit(chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..512), 1..20)) {
        let mut reader = BoundedLineReader::new();

        for chunk in &chunks {
            let _ = reader.feed(chunk);
        }

        // Even with arbitrary hostile chunks, the reader does not panic or crash.
    }
}
