#![no_main]

use kynex_apps_index::exec::{parse_exec, ExecContext};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    let files = ["/tmp/file1.txt", "/tmp/file2.txt"];
    let urls = ["https://example.com/a", "file:///tmp/b"];
    let ctx = ExecContext {
        name: "Fuzz App",
        icon: "test-icon",
        file_path: "/usr/share/applications/fuzz.desktop",
        files: &files,
        urls: &urls,
    };
    let _ = parse_exec(&s, &ctx);
});
