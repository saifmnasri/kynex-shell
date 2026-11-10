//! Hostile corpus image decoding tests (M1.3).
//!
//! Verifies:
//! - PNG decompression bomb header (60000x60000) is rejected at header check without decoding.
//! - Truncated PNG is safely rejected.
//! - SVG with external file references (`file:///etc/passwd`) is rejected.
//! - SVG with external DTD/entity declarations is rejected.
//! - SVG with nested `<use>` expansion bomb (> 64 tags) is rejected.
//! - Valid PNG and SVG decode successfully within bounds.

#![allow(clippy::unwrap_used, clippy::needless_raw_string_hashes)]

use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

use kynex_shell::shim::{is_svg_safe, test_decode_icon};

#[test]
fn test_svg_security_filter() {
    // 1. External file reference
    let evil_file_svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
        <image href="file:///etc/passwd" width="32" height="32" />
    </svg>"#;
    assert!(
        !is_svg_safe(evil_file_svg),
        "SVG with file:/// URI must be rejected"
    );

    // 2. External DTD
    let evil_dtd_svg = br#"<?xml version="1.0"?>
    <!DOCTYPE svg SYSTEM "http://example.com/evil.dtd">
    <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"></svg>"#;
    assert!(
        !is_svg_safe(evil_dtd_svg),
        "SVG with DOCTYPE must be rejected"
    );

    // 3. External entity
    let evil_entity_svg = br#"<?xml version="1.0"?>
    <!DOCTYPE svg [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
    <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><text>&xxe;</text></svg>"#;
    assert!(
        !is_svg_safe(evil_entity_svg),
        "SVG with ENTITY must be rejected"
    );

    // 4. Use expansion bomb (> 64 use elements)
    let mut bomb_svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><g id="a"><circle r="1"/></g></defs>"##,
    );
    for _ in 0..70 {
        bomb_svg.push_str(r##"<use href="#a"/>"##);
    }
    bomb_svg.push_str("</svg>");
    assert!(
        !is_svg_safe(bomb_svg.as_bytes()),
        "SVG with >64 <use> tags must be rejected"
    );

    // 5. Valid embedded SVG
    let safe_svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
        <rect width="32" height="32" fill="#ff0000" />
    </svg>"##;
    assert!(is_svg_safe(safe_svg), "Clean SVG must be accepted");
}

fn calc_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xedb8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[test]
fn test_png_decompression_bomb_header_rejected() {
    let tmp = TempDir::new().unwrap();
    let bomb_path = tmp.path().join("bomb_60000x60000.png");

    // Construct valid PNG header with 60000 x 60000 dimensions
    // PNG signature (8 bytes)
    let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    // IHDR chunk: length = 13 (4 bytes)
    data.extend_from_slice(&13u32.to_be_bytes());
    // Chunk type: "IHDR"
    let ihdr_start = data.len();
    data.extend_from_slice(b"IHDR");
    // Width = 60000 (0x0000EA60)
    data.extend_from_slice(&60000u32.to_be_bytes());
    // Height = 60000 (0x0000EA60)
    data.extend_from_slice(&60000u32.to_be_bytes());
    // Bit depth = 8, Color type = 2 (Truecolor), Compression = 0, Filter = 0, Interlace = 0
    data.extend_from_slice(&[8, 2, 0, 0, 0]);
    // CRC32 of IHDR data
    let crc = calc_crc32(&data[ihdr_start..]);
    data.extend_from_slice(&crc.to_be_bytes());

    File::create(&bomb_path).unwrap().write_all(&data).unwrap();

    let decoded = test_decode_icon(bomb_path.to_str().unwrap(), 32, 32, 1);
    assert!(
        !decoded,
        "60000x60000 PNG decompression bomb must be rejected at header check"
    );
}

#[test]
fn test_truncated_png_rejected() {
    let tmp = TempDir::new().unwrap();
    let trunc_path = tmp.path().join("truncated.png");

    // Partial PNG header only
    let data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A];
    File::create(&trunc_path).unwrap().write_all(&data).unwrap();

    let decoded = test_decode_icon(trunc_path.to_str().unwrap(), 32, 32, 1);
    assert!(!decoded, "Truncated PNG must fail safely without panic");
}

#[test]
fn test_hostile_svg_decoding_rejected() {
    let tmp = TempDir::new().unwrap();
    let evil_svg_path = tmp.path().join("evil.svg");

    let evil_svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
        <image href="file:///etc/passwd" width="32" height="32" />
    </svg>"#;
    File::create(&evil_svg_path)
        .unwrap()
        .write_all(evil_svg)
        .unwrap();

    let decoded = test_decode_icon(evil_svg_path.to_str().unwrap(), 32, 32, 1);
    assert!(
        !decoded,
        "Hostile SVG referencing /etc/passwd must fail decoding"
    );
}

#[test]
fn test_valid_svg_decodes_cleanly() {
    let tmp = TempDir::new().unwrap();
    let safe_svg_path = tmp.path().join("valid.svg");

    let safe_svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
        <rect width="32" height="32" fill="#00ff00" />
    </svg>"##;
    File::create(&safe_svg_path)
        .unwrap()
        .write_all(safe_svg)
        .unwrap();

    let decoded = test_decode_icon(safe_svg_path.to_str().unwrap(), 32, 32, 1);
    assert!(decoded, "Valid clean SVG must decode successfully");
}
