#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::cloned_ref_to_slice_refs,
    clippy::assert_is_empty,
    clippy::single_char_pattern,
    clippy::cast_possible_truncation
)]
//! Property-based tests for desktop entry parser and Exec expansion.

use proptest::prelude::*;
use std::path::Path;

use kynex_apps_index::entry::DesktopEntry;
use kynex_apps_index::exec::{parse_exec, ExecContext};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_parse_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..8192)) {
        let _ = DesktopEntry::parse_bytes("test.desktop", Path::new("test.desktop"), &bytes, "Kynex");
    }

    #[test]
    fn prop_parse_exec_never_panics(exec_line in "\\PC*") {
        let files = ["/tmp/file1", "/tmp/file2"];
        let urls = ["https://example.com/1", "https://example.com/2"];
        let ctx = ExecContext {
            name: "Test App",
            icon: "test-icon",
            file_path: "/usr/share/applications/test.desktop",
            files: &files,
            urls: &urls,
        };
        let _ = parse_exec(&exec_line, &ctx);
    }

    #[test]
    fn prop_argv_never_contains_unexpanded_field_codes(exec_line in "\\PC*") {
        let files = ["/tmp/f1"];
        let urls = ["http://u1"];
        let ctx = ExecContext {
            name: "AppName",
            icon: "icon",
            file_path: "/path/to.desktop",
            files: &files,
            urls: &urls,
        };

        if let Ok(argv) = parse_exec(&exec_line, &ctx) {
            // When there is at most one '%' in the input (cannot be an escaped '%%'),
            // argument tokens (argv[1..]) must NEVER contain any unexpanded field codes.
            // (Note: argv[0] is the command binary, which per spec is not subject to field code expansion).
            if exec_line.matches('%').count() <= 1 && argv.len() > 1 {
                let unexpanded = [
                    "%f", "%F", "%u", "%U", "%i", "%c", "%k",
                    "%d", "%D", "%n", "%N", "%v", "%m",
                ];
                for arg in &argv[1..] {
                    for code in unexpanded {
                        assert!(
                            !arg.contains(code),
                            "Found unexpanded field code {code} in argv token '{arg}' from input '{exec_line}'"
                        );
                    }
                }
            }
        }
    }
}
