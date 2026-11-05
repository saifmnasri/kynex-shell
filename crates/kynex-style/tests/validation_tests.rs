//! Unit and integration tests for style pack validation and malicious manifest rejection.

#![allow(clippy::expect_used, clippy::panic, clippy::needless_raw_string_hashes)]

use kynex_style::{load_style_pack, read_file_with_limit, StyleError, MAX_STYLE_FILE_SIZE};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

fn write_file(dir: &Path, rel_path: &str, content: &str) {
    let p = dir.join(rel_path);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent dir");
    }
    let mut f = File::create(&p).expect("failed to create file");
    f.write_all(content.as_bytes())
        .expect("failed to write content");
}

const VALID_STYLE: &str = r##"
schema_version = 1
id = "test-pack"
name = "Test Pack"
version = "0.1.0"
author = "Kynex Contributors"
license = "MIT"
kind = "full"
archetype = "dock"
min_shell_api = 1

[entry]
shell = "Shell.qml"

[tokens]
file = "tokens.toml"
"##;

const VALID_TOKENS: &str = r##"
schema_version = 1

[color.dark]
surface = "#14161c"
surface_raised = "#1d2029"
text = "#eceff4"
text_muted = "#9aa3b2"
accent = "#6aa6ff"
danger = "#ff6b6b"
success = "#4cd1a0"

[color.light]
surface = "#f6f7fa"
surface_raised = "#ffffff"
text = "#14161c"
text_muted = "#5b6475"
accent = "#2f6fe0"
danger = "#d63c3c"
success = "#1f9d73"

[spacing]
unit = 4

[radius]
small = 6
medium = 12
large = 20

[motion]
fast_ms = 90
normal_ms = 150
slow_ms = 240

[typography]
family = "Inter"
fallback = "sans-serif"
base_px = 14
"##;

#[test]
fn test_default_style_pack_validates() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("parent")
        .parent()
        .expect("repo root");
    let default_pack = repo_root.join("styles").join("default");
    let (manifest, tokens) =
        load_style_pack(&default_pack).expect("default style pack must be valid");
    assert_eq!(manifest.id, "default");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.entry.shell, "Shell.qml");
    assert_eq!(tokens.schema_version, 1);
    assert_eq!(tokens.color.dark.surface, "#14161c");
}

#[test]
fn test_valid_style_pack_loads_successfully() {
    let dir = tempdir().expect("tempdir");
    write_file(dir.path(), "style.toml", VALID_STYLE);
    write_file(dir.path(), "tokens.toml", VALID_TOKENS);
    write_file(dir.path(), "Shell.qml", "// test shell");

    let (manifest, tokens) = load_style_pack(dir.path()).expect("should load valid pack");
    assert_eq!(manifest.id, "test-pack");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(tokens.color.dark.accent, "#6aa6ff");
    assert_eq!(tokens.radius.medium, 12);
}

#[test]
fn test_malicious_path_traversal_in_entry() {
    let dir = tempdir().expect("tempdir");
    let malicious = VALID_STYLE.replace(r#"shell = "Shell.qml""#, r#"shell = "../../etc/shadow""#);
    write_file(dir.path(), "style.toml", &malicious);
    write_file(dir.path(), "tokens.toml", VALID_TOKENS);

    let err = load_style_pack(dir.path()).expect_err("should reject path traversal");
    match err {
        StyleError::UnsafePath { context, path } => {
            assert_eq!(context, "entry.shell");
            assert!(path.contains(".."));
        }
        other => panic!("expected UnsafePath, got: {other:?}"),
    }
}

#[test]
fn test_malicious_path_traversal_in_tokens() {
    let dir = tempdir().expect("tempdir");
    let malicious = VALID_STYLE.replace(r#"file = "tokens.toml""#, r#"file = "../tokens.toml""#);
    write_file(dir.path(), "style.toml", &malicious);
    write_file(dir.path(), "Shell.qml", "// shell");

    let err = load_style_pack(dir.path()).expect_err("should reject path traversal");
    match err {
        StyleError::UnsafePath { context, .. } => {
            assert_eq!(context, "tokens.file");
        }
        other => panic!("expected UnsafePath, got: {other:?}"),
    }
}

#[test]
fn test_malicious_absolute_path_rejected() {
    let dir = tempdir().expect("tempdir");
    let malicious = VALID_STYLE.replace(r#"shell = "Shell.qml""#, r#"shell = "/etc/passwd""#);
    write_file(dir.path(), "style.toml", &malicious);
    write_file(dir.path(), "tokens.toml", VALID_TOKENS);

    let err = load_style_pack(dir.path()).expect_err("should reject absolute path");
    match err {
        StyleError::UnsafePath { context, path } => {
            assert_eq!(context, "entry.shell");
            assert_eq!(path, "/etc/passwd");
        }
        other => panic!("expected UnsafePath, got: {other:?}"),
    }
}

#[test]
fn test_unsupported_schema_version_rejected() {
    let dir = tempdir().expect("tempdir");
    let malicious = VALID_STYLE.replace("schema_version = 1", "schema_version = 99");
    write_file(dir.path(), "style.toml", &malicious);
    write_file(dir.path(), "tokens.toml", VALID_TOKENS);
    write_file(dir.path(), "Shell.qml", "// shell");

    let err = load_style_pack(dir.path()).expect_err("should reject schema version 99");
    match err {
        StyleError::UnsupportedSchemaVersion {
            version, expected, ..
        } => {
            assert_eq!(version, 99);
            assert_eq!(expected, 1);
        }
        other => panic!("expected UnsupportedSchemaVersion, got: {other:?}"),
    }
}

#[test]
fn test_invalid_style_id_rejected() {
    let dir = tempdir().expect("tempdir");
    let malicious = VALID_STYLE.replace(r#"id = "test-pack""#, r#"id = "INVALID_ID!""#);
    write_file(dir.path(), "style.toml", &malicious);
    write_file(dir.path(), "tokens.toml", VALID_TOKENS);
    write_file(dir.path(), "Shell.qml", "// shell");

    let err = load_style_pack(dir.path()).expect_err("should reject invalid id");
    match err {
        StyleError::InvalidId { id } => {
            assert_eq!(id, "INVALID_ID!");
        }
        other => panic!("expected InvalidId, got: {other:?}"),
    }
}

#[test]
fn test_invalid_color_in_tokens_rejected() {
    let dir = tempdir().expect("tempdir");
    write_file(dir.path(), "style.toml", VALID_STYLE);
    let bad_tokens = VALID_TOKENS.replace(r##"accent = "#6aa6ff""##, r#"accent = "not-a-color""#);
    write_file(dir.path(), "tokens.toml", &bad_tokens);
    write_file(dir.path(), "Shell.qml", "// shell");

    let err = load_style_pack(dir.path()).expect_err("should reject invalid color");
    match err {
        StyleError::InvalidColor { color } => {
            assert_eq!(color, "not-a-color");
        }
        other => panic!("expected InvalidColor, got: {other:?}"),
    }
}

#[test]
fn test_oversized_file_rejected() {
    let dir = tempdir().expect("tempdir");
    let oversized_path = dir.path().join("oversized.toml");
    let mut f = File::create(&oversized_path).expect("create");
    // Write 1 MB + 1 byte
    let chunk = vec![b'a'; 1024];
    for _ in 0..1025 {
        f.write_all(&chunk).expect("write");
    }

    let err = read_file_with_limit(&oversized_path, MAX_STYLE_FILE_SIZE)
        .expect_err("should reject oversized file");
    match err {
        StyleError::FileTooLarge {
            found_bytes,
            max_bytes,
            ..
        } => {
            assert!(found_bytes > max_bytes);
            assert_eq!(max_bytes, MAX_STYLE_FILE_SIZE);
        }
        other => panic!("expected FileTooLarge, got: {other:?}"),
    }
}

#[test]
fn test_symlink_escaping_pack_rejected() {
    let parent = tempdir().expect("parent");
    let pack = parent.path().join("pack");
    std::fs::create_dir_all(&pack).expect("create pack dir");

    let secret_file = parent.path().join("secret.txt");
    std::fs::write(&secret_file, "secret").expect("write secret");

    // Create a symlink inside the pack pointing to secret_file outside the pack
    #[cfg(unix)]
    std::os::unix::fs::symlink(&secret_file, pack.join("Shell.qml")).expect("create symlink");

    write_file(&pack, "style.toml", VALID_STYLE);
    write_file(&pack, "tokens.toml", VALID_TOKENS);

    let err = load_style_pack(&pack).expect_err("should reject symlink leaving pack");
    match err {
        StyleError::UnsafePath { context, .. } => {
            assert!(context.contains("symlink target"));
        }
        other => panic!("expected UnsafePath, got: {other:?}"),
    }
}

#[test]
fn test_fifo_rejected_without_hanging() {
    let dir = tempdir().expect("tempdir");
    let fifo_path = dir.path().join("fifo.pipe");

    // Create a FIFO via mkfifo command
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status()
        .expect("execute mkfifo");
    assert!(status.success(), "mkfifo should succeed");

    let err = read_file_with_limit(&fifo_path, MAX_STYLE_FILE_SIZE)
        .expect_err("should reject FIFO before opening");
    match err {
        StyleError::NotARegularFile { path } => {
            assert_eq!(path, fifo_path);
        }
        other => panic!("expected NotARegularFile, got: {other:?}"),
    }
}

#[test]
fn test_symlink_to_dev_zero_rejected_without_hanging() {
    let dir = tempdir().expect("tempdir");
    let symlink_path = dir.path().join("zero.toml");

    #[cfg(unix)]
    std::os::unix::fs::symlink("/dev/zero", &symlink_path).expect("create symlink to /dev/zero");

    let err = read_file_with_limit(&symlink_path, MAX_STYLE_FILE_SIZE)
        .expect_err("should reject character device /dev/zero");
    match err {
        StyleError::NotARegularFile { path } => {
            assert_eq!(path, symlink_path);
        }
        other => panic!("expected NotARegularFile, got: {other:?}"),
    }
}
