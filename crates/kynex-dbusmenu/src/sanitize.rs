//! Input sanitization and formatting routines for untrusted dbusmenu data.

use crate::limits::{MAX_ICON_NAME_LEN, MAX_LABEL_LEN, MAX_SHORTCUT_LEN};
use crate::raw::RawValue;

/// Checks if a character is a Unicode bidirectional override, isolate, or embedding character.
#[must_use]
pub fn is_bidi_override(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    )
}

/// Checks if a character is an ASCII control character (0x00..=0x1F or 0x7F).
#[must_use]
pub fn is_control_char(c: char) -> bool {
    matches!(c, '\0'..='\x1F' | '\x7F')
}

/// Strips HTML/XML tags from an input string.
#[must_use]
pub fn strip_markup(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;

    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }

    out
}

/// Parses mnemonic underscores according to standard menu rules:
/// - `_File` becomes `File` (accelerator marker stripped).
/// - `__` becomes literal `_`.
/// - Trailing lone `_` is dropped.
#[must_use]
pub fn parse_mnemonic_underscores(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '_' {
            if let Some(&'_') = chars.peek() {
                // Escaped literal underscore
                out.push('_');
                chars.next();
            }
            // Otherwise: mnemonic accelerator indicator (keep following char) or trailing lone underscore (dropped)
        } else {
            out.push(c);
        }
    }

    out
}

/// Sanitizes an untrusted menu label:
/// 1. Strips HTML/XML markup tags.
/// 2. Handles mnemonic underscores (`_File` -> `File`, `__` -> `_`).
/// 3. Drops ASCII control characters and Unicode bidi overrides/isolates.
/// 4. Clamps output length to at most [`MAX_LABEL_LEN`] (256) characters sliced on UTF-8 boundaries.
#[must_use]
pub fn sanitize_label(input: &str) -> String {
    let stripped = strip_markup(input);
    let mnemonic_parsed = parse_mnemonic_underscores(&stripped);

    let filtered: String = mnemonic_parsed
        .chars()
        .filter(|&c| !is_control_char(c) && !is_bidi_override(c))
        .collect();

    // Clamp to at most MAX_LABEL_LEN characters on UTF-8 boundaries
    filtered.chars().take(MAX_LABEL_LEN).collect()
}

/// Validates an icon name against Freedesktop icon specification rules:
/// - Maximum [`MAX_ICON_NAME_LEN`] (128) characters.
/// - Only alphanumeric characters, dashes, underscores, and dots.
/// - Prohibits slashes, directory traversal (`..`), and paths.
#[must_use]
pub fn sanitize_icon_name(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_ICON_NAME_LEN
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        return String::new();
    }

    let is_valid = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');

    if is_valid {
        trimmed.to_string()
    } else {
        String::new()
    }
}

/// Normalizes key and modifier names for keyboard shortcuts.
fn normalize_key_component(token: &str) -> &str {
    match token {
        "Control" | "ctrl" | "Control_L" | "Control_R" => "Ctrl",
        "Alt" | "alt" | "Alt_L" | "Alt_R" | "Mod1" => "Alt",
        "Shift" | "shift" | "Shift_L" | "Shift_R" => "Shift",
        "Super" | "super" | "Super_L" | "Super_R" | "Mod4" => "Super",
        "Return" | "Enter" => "Enter",
        "Escape" | "Esc" => "Esc",
        "BackSpace" => "Backspace",
        "Tab" => "Tab",
        "space" | "Space" => "Space",
        other => other,
    }
}

/// Formats and sanitizes a keyboard shortcut variant into a readable text representation:
/// Handles string representation (`"Ctrl+Q"`), array of tokens (`["Control", "q"]`),
/// and array of arrays (`[["Control", "q"]]`).
#[must_use]
pub fn format_and_sanitize_shortcut(val: &RawValue) -> String {
    let raw_text = match val {
        RawValue::String(s) => s.clone(),
        RawValue::Array(outer) => {
            if outer.is_empty() {
                String::new()
            } else if let Some(inner) = outer.first().and_then(|v| v.as_array()) {
                // Nested array: take first key combo
                format_shortcut_tokens(inner)
            } else {
                format_shortcut_tokens(outer)
            }
        }
        _ => String::new(),
    };

    let filtered: String = raw_text
        .chars()
        .filter(|&c| !is_control_char(c) && !is_bidi_override(c))
        .collect();

    filtered.chars().take(MAX_SHORTCUT_LEN).collect()
}

fn format_shortcut_tokens(tokens: &[RawValue]) -> String {
    let mut parts = Vec::new();
    for token in tokens {
        if let Some(s) = token.as_str() {
            let normalized = normalize_key_component(s.trim());
            if !normalized.is_empty() {
                if normalized.len() == 1 {
                    parts.push(normalized.to_uppercase());
                } else {
                    parts.push(normalized.to_string());
                }
            }
        }
    }
    parts.join("+")
}
