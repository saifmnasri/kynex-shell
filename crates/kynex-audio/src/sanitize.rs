//! Device name sanitization and character filtering.

use crate::limits::MAX_DEVICE_NAME_CHARS;

/// Checks whether a character is an ASCII or C1 control character.
fn is_forbidden_control(ch: char) -> bool {
    let u = ch as u32;
    (u <= 0x1F) || (u == 0x7F) || (0x80..=0x9F).contains(&u)
}

/// Checks whether a character is a Unicode bidirectional formatting or isolate override.
fn is_bidi_override(ch: char) -> bool {
    matches!(
        ch,
        '\u{202A}' // Left-to-Right Embedding (LRE)
        | '\u{202B}' // Right-to-Left Embedding (RLE)
        | '\u{202C}' // Pop Directional Formatting (PDF)
        | '\u{202D}' // Left-to-Right Override (LRO)
        | '\u{202E}' // Right-to-Left Override (RLO)
        | '\u{2066}' // Left-to-Right Isolate (LRI)
        | '\u{2067}' // Right-to-Left Isolate (RLI)
        | '\u{2068}' // First Strong Isolate (FSI)
        | '\u{2069}' // Pop Directional Isolate (PDI)
        | '\u{200E}' // Left-to-Right Mark (LRM)
        | '\u{200F}' // Right-to-Left Mark (RLM)
        | '\u{061C}' // Arabic Letter Mark (ALM)
    )
}

/// Sanitizes an audio device name or description string according to security invariants.
///
/// Drops ASCII controls (0x00-0x1F, 0x7F), C1 controls (0x80-0x9F), and Unicode bidirectional
/// overrides/isolates (U+202A-202E, U+2066-2069, U+200E, U+200F, U+061C), trims whitespace,
/// and clamps length to at most 128 characters.
#[must_use]
pub fn sanitize_device_name(input: &str) -> String {
    let mut filtered = String::with_capacity(input.len());
    let mut char_count: usize = 0;

    for ch in input.chars() {
        if is_bidi_override(ch) || is_forbidden_control(ch) {
            continue;
        }
        if char_count >= MAX_DEVICE_NAME_CHARS {
            break;
        }
        filtered.push(ch);
        char_count = char_count.saturating_add(1);
    }

    filtered.trim().to_string()
}
