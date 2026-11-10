//! Property-based tests for `index.theme` parsing and icon name validation.

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use std::io::Cursor;
use std::path::Path;

use kynex_icons::lookup::validate_icon_name;
use kynex_icons::theme::ThemeIndex;

proptest! {
    /// Property: `validate_icon_name` must never panic on any arbitrary UTF-8 string,
    /// and must only return Ok if length <= 128 and chars are within [a-zA-Z0-9._-].
    #[test]
    fn prop_validate_icon_name(name in "\\PC*") {
        let res = validate_icon_name(&name);
        if let Ok(valid) = res {
            prop_assert!(!valid.is_empty());
            prop_assert!(valid.len() <= 128);
            prop_assert!(!valid.contains('\0'));
            prop_assert!(!valid.contains('/'));
            prop_assert!(!valid.contains('\\'));
            prop_assert!(!valid.contains(".."));
            for b in valid.bytes() {
                prop_assert!(b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
            }
        }
    }

    /// Property: `ThemeIndex::parse_reader` must never panic or loop indefinitely on arbitrary byte sequences.
    #[test]
    fn prop_theme_index_parser(bytes in prop::collection::vec(any::<u8>(), 0..8192)) {
        let cursor = Cursor::new(bytes);
        let path = Path::new("/mock/index.theme");
        let _ = ThemeIndex::parse_reader(cursor, path);
    }

    /// Property: Structured pseudo-INI strings must parse without panicking.
    #[test]
    fn prop_theme_index_structured(
        name in "[a-zA-Z0-9_-]{1,32}",
        inherits in "[a-zA-Z0-9_,-]{0,64}",
        size in 1u32..512u32,
        scale in 1u32..4u32,
        dir_type in "(Fixed|Scalable|Threshold)",
    ) {
        let content = format!(
            "[Icon Theme]\nName={name}\nInherits={inherits}\nDirectories=test_dir\n\n[test_dir]\nSize={size}\nScale={scale}\nType={dir_type}\n"
        );
        let cursor = Cursor::new(content.into_bytes());
        let path = Path::new("/mock/index.theme");
        let res = ThemeIndex::parse_reader(cursor, path);
        prop_assert!(res.is_ok());
        let parsed = res.unwrap();
        prop_assert_eq!(parsed.name, name);
        prop_assert_eq!(parsed.directories.len(), 1);
        prop_assert_eq!(parsed.directories[0].size, size);
        prop_assert_eq!(parsed.directories[0].scale, scale);
    }
}
