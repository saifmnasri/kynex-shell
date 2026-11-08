#![no_main]

use kynex_apps_index::entry::DesktopEntry;
use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    let path = Path::new("/usr/share/applications/fuzz.desktop");
    let _ = DesktopEntry::parse_bytes("fuzz.desktop", path, data, "Kynex");
});

