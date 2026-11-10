#![no_main]

use std::io::Cursor;
use std::path::Path;

use kynex_icons::lookup::validate_icon_name;
use kynex_icons::theme::ThemeIndex;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // 1. Fuzz icon name validation
    if let Ok(name) = std::str::from_utf8(data) {
        let _ = validate_icon_name(name);
    }

    // 2. Fuzz index.theme parsing
    let cursor = Cursor::new(data);
    let path = Path::new("/fuzz/index.theme");
    let _ = ThemeIndex::parse_reader(cursor, path);
});

