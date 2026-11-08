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
//! Unit tests for in-house ranked search matcher.

use kynex_apps_index::entry::DesktopEntry;
use kynex_apps_index::search::search_entries;
use std::path::Path;

fn make_entry(
    id: &str,
    name: &str,
    generic: &str,
    keywords: &[&str],
    categories: &[&str],
) -> DesktopEntry {
    let mut e = DesktopEntry::parse_bytes(
        id,
        Path::new(id),
        format!("[Desktop Entry]\nType=Application\nName={name}\nExec=cmd\n").as_bytes(),
        "Kynex",
    )
    .unwrap();
    e.generic_name.default = generic.to_string();
    e.keywords = keywords.iter().map(|&s| s.to_string()).collect();
    e.categories = categories.iter().map(|&s| s.to_string()).collect();
    e
}

#[test]
fn test_search_ranking_hierarchy() {
    let entries = vec![
        make_entry(
            "term.desktop",
            "Terminal",
            "Console",
            &["shell", "prompt"],
            &["System"],
        ),
        make_entry(
            "konsole.desktop",
            "Konsole",
            "Terminal Emulator",
            &["terminal", "kde"],
            &["System"],
        ),
        make_entry(
            "browser.desktop",
            "Web Browser",
            "Internet",
            &["www", "surf"],
            &["Network"],
        ),
        make_entry(
            "settings.desktop",
            "Terminal Settings",
            "Configuration",
            &[],
            &["Settings"],
        ),
    ];

    // Query "term"
    // "Terminal" starts with "term" in Name (exact prefix) -> high score
    // "Terminal Settings" starts with "term" in Name
    // "Konsole" has "terminal" in GenericName & Keywords
    let results = search_entries(&entries, "term", 10, None);
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "term.desktop");
    assert_eq!(results[1].id, "konsole.desktop");
    assert_eq!(results[2].id, "settings.desktop");
}

#[test]
fn test_search_subsequence_and_word_start() {
    let entries = vec![
        make_entry(
            "vlc.desktop",
            "VLC Media Player",
            "Video Player",
            &[],
            &["AudioVideo"],
        ),
        make_entry("mpv.desktop", "mpv", "Video Player", &[], &["AudioVideo"]),
        make_entry("calc.desktop", "Calculator", "Math", &[], &["Utility"]),
    ];

    // Query "vmp" matches "VLC Media Player" via word start ("V", "M", "P")
    let results = search_entries(&entries, "vmp", 5, None);
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "vlc.desktop");
}

#[test]
fn test_empty_query_returns_alphabetical() {
    let entries = vec![
        make_entry("c.desktop", "Charlie", "", &[], &[]),
        make_entry("a.desktop", "Alpha", "", &[], &[]),
        make_entry("b.desktop", "Bravo", "", &[], &[]),
    ];

    let results = search_entries(&entries, "", 10, None);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].id, "a.desktop");
    assert_eq!(results[1].id, "b.desktop");
    assert_eq!(results[2].id, "c.desktop");
}

#[test]
fn test_search_with_locale() {
    let mut e1 = make_entry("calc.desktop", "Calculator", "Math", &[], &["Utility"]);
    e1.name
        .localized
        .insert("fr".to_string(), "Calculatrice".to_string());
    let entries = vec![e1];

    let results_fr = search_entries(&entries, "Calculatrice", 5, Some("fr"));
    assert_eq!(results_fr.len(), 1);
    assert_eq!(results_fr[0].id, "calc.desktop");

    let results_en = search_entries(&entries, "Calculatrice", 5, Some("en"));
    assert_eq!(results_en.len(), 0);
}

#[test]
fn test_search_edge_cases_and_tie_breaking() {
    let entries = vec![
        make_entry("t1.desktop", "Terminal", "", &[], &[]),
        make_entry("t2.desktop", "Terminal", "", &[], &[]),
    ];

    // Limit 0 defaults to cap 20
    let def_limit = search_entries(&entries, "term", 0, None);
    assert_eq!(def_limit.len(), 2);

    // Query exceeding 128 characters returns empty
    let oversized = "a".repeat(150);
    let empty2 = search_entries(&entries, &oversized, 5, None);
    assert!(empty2.is_empty());

    // Tie breaking on identical names
    let tie = search_entries(&entries, "Terminal", 5, None);
    assert_eq!(tie.len(), 2);
}
