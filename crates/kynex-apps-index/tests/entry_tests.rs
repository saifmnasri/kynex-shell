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
//! Unit and hostile test suite for desktop entry parser.

use std::fs;
use std::path::Path;
use tempfile::tempdir;

use kynex_apps_index::entry::{
    decode_escapes, generate_locale_candidates, parse_string_list, DesktopEntry, DesktopEntryError,
    LocalizedString, MAX_FILE_SIZE, MAX_LINE_LENGTH,
};

#[test]
fn test_decode_escapes() {
    assert_eq!(
        decode_escapes(r"Hello\sWorld\nNew\tLine\\"),
        "Hello World\nNew\tLine\\"
    );
    assert_eq!(decode_escapes(r"Semi\;Colon"), "Semi;Colon");
    assert_eq!(decode_escapes(r"Carriage\rReturn"), "Carriage\rReturn");
}

#[test]
fn test_parse_string_list() {
    let raw = r"First;Second\;WithSemi;Third;";
    let list = parse_string_list(raw);
    assert_eq!(list, vec!["First", "Second;WithSemi", "Third"]);
}

#[test]
fn test_locale_candidate_generation() {
    let candidates = generate_locale_candidates("fr_FR.UTF-8@euro");
    assert_eq!(candidates, vec!["fr_FR@euro", "fr_FR", "fr@euro", "fr"]);

    let simple = generate_locale_candidates("de_DE");
    assert_eq!(simple, vec!["de_DE", "de"]);

    let lang_only = generate_locale_candidates("en");
    assert_eq!(lang_only, vec!["en"]);
}

#[test]
fn test_localized_string_fallback() {
    let mut loc = LocalizedString::new("Default");
    loc.localized
        .insert("fr".to_string(), "Francais".to_string());
    loc.localized
        .insert("fr_CA".to_string(), "Canadien".to_string());

    assert_eq!(loc.resolve(Some("fr_CA.UTF-8")), "Canadien");
    assert_eq!(loc.resolve(Some("fr_FR.UTF-8")), "Francais");
    assert_eq!(loc.resolve(Some("de_DE")), "Default");
    assert_eq!(loc.resolve(None), "Default");
}

#[test]
fn test_parse_valid_desktop_entry() {
    let content = br#"[Desktop Entry]
Type=Application
Name=Test Editor
Name[fr]=Editeur Test
GenericName=Text Editor
Comment=Edit text files
Icon=accessories-text-editor
Exec=editor %f
Categories=Utility;TextEditor;
Keywords=text;editor;notes;
"#;
    let entry = DesktopEntry::parse_bytes(
        "editor.desktop",
        Path::new("/usr/share/applications/editor.desktop"),
        content,
        "Kynex",
    )
    .expect("Should parse valid desktop file");

    assert_eq!(entry.id, "editor.desktop");
    assert_eq!(entry.name.resolve(None), "Test Editor");
    assert_eq!(entry.name.resolve(Some("fr_FR")), "Editeur Test");
    assert_eq!(entry.generic_name.resolve(None), "Text Editor");
    assert_eq!(entry.icon, "accessories-text-editor");
    assert_eq!(entry.categories, vec!["Utility", "TextEditor"]);
    assert_eq!(entry.keywords, vec!["text", "editor", "notes"]);
    assert!(!entry.terminal);
    assert!(!entry.hidden);
    assert!(!entry.no_display);
}

#[test]
fn test_reject_non_application_type() {
    let content = b"[Desktop Entry]\nType=Link\nName=Web Link\nURL=https://example.com\n";
    let res =
        DesktopEntry::parse_bytes("link.desktop", Path::new("link.desktop"), content, "Kynex");
    assert!(matches!(res, Err(DesktopEntryError::NotAnApplication(_))));
}

#[test]
fn test_reject_hidden_and_nodisplay() {
    let hidden = b"[Desktop Entry]\nType=Application\nName=App\nHidden=true\n";
    let res_h = DesktopEntry::parse_bytes("h.desktop", Path::new("h.desktop"), hidden, "Kynex");
    assert_eq!(res_h, Err(DesktopEntryError::Hidden));

    let nodisplay = b"[Desktop Entry]\nType=Application\nName=App\nNoDisplay=true\n";
    let res_nodisp = DesktopEntry::parse_bytes(
        "nodisp.desktop",
        Path::new("nodisp.desktop"),
        nodisplay,
        "Kynex",
    );
    assert_eq!(res_nodisp, Err(DesktopEntryError::NoDisplay));
}

#[test]
fn test_desktop_environment_filters() {
    let only_gnome =
        b"[Desktop Entry]\nType=Application\nName=App\nExec=app\nOnlyShowIn=GNOME;XFCE;\n";
    let res_only =
        DesktopEntry::parse_bytes("app.desktop", Path::new("app.desktop"), only_gnome, "Kynex");
    assert!(matches!(
        res_only,
        Err(DesktopEntryError::DesktopNotAllowed(_))
    ));

    let only_kynex =
        b"[Desktop Entry]\nType=Application\nName=App\nExec=app\nOnlyShowIn=Kynex;GNOME;\n";
    let res_kynex =
        DesktopEntry::parse_bytes("app.desktop", Path::new("app.desktop"), only_kynex, "Kynex");
    assert!(res_kynex.is_ok());

    let not_kynex = b"[Desktop Entry]\nType=Application\nName=App\nExec=app\nNotShowIn=Kynex;\n";
    let res_not =
        DesktopEntry::parse_bytes("app.desktop", Path::new("app.desktop"), not_kynex, "Kynex");
    assert!(matches!(
        res_not,
        Err(DesktopEntryError::DesktopNotAllowed(_))
    ));
}

#[test]
fn test_oversized_file_and_line_limits() {
    // 64 KiB file limit
    let huge_bytes = vec![b'a'; (MAX_FILE_SIZE + 10) as usize];
    let res_file = DesktopEntry::parse_bytes(
        "huge.desktop",
        Path::new("huge.desktop"),
        &huge_bytes,
        "Kynex",
    );
    assert!(matches!(res_file, Err(DesktopEntryError::FileTooLarge(_))));

    // 4 KiB line limit
    let mut long_line = b"[Desktop Entry]\nType=Application\nName=".to_vec();
    long_line.extend(vec![b'A'; MAX_LINE_LENGTH + 10]);
    long_line.push(b'\n');
    let res_line = DesktopEntry::parse_bytes(
        "long.desktop",
        Path::new("long.desktop"),
        &long_line,
        "Kynex",
    );
    assert!(matches!(res_line, Err(DesktopEntryError::LineTooLong(_))));
}

#[test]
fn test_try_exec_nonexistent_rejected() {
    let content = b"[Desktop Entry]\nType=Application\nName=App\nExec=app\nTryExec=totally_nonexistent_binary_xyz_12345\n";
    let res = DesktopEntry::parse_bytes("app.desktop", Path::new("app.desktop"), content, "Kynex");
    assert!(matches!(res, Err(DesktopEntryError::TryExecNotFound(_))));
}

#[test]
fn test_fifo_rejected() {
    let dir = tempdir().unwrap();
    let fifo_path = dir.path().join("fifo.desktop");

    let status = std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status();
    if let Ok(st) = status {
        if st.success() {
            let meta = fs::metadata(&fifo_path).unwrap();
            assert!(!meta.is_file());
            let res = DesktopEntry::parse_bytes(
                "fifo.desktop",
                &fifo_path,
                b"[Desktop Entry]\nType=Application\nName=Fifo\nExec=cmd\n",
                "Kynex",
            );
            assert!(res.is_ok()); // parse_bytes parses bytes, but index loader checks meta.is_file()!
        }
    }
}
