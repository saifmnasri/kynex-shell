//! Input sanitization, markup stripping, control/bidi character filtering,
//! and identifier validation for untrusted StatusNotifierItem payloads.

use crate::limits::{
    MAX_ICON_NAME_CHARS, MAX_ID_CHARS, MAX_TITLE_CHARS, MAX_TOOLTIP_TEXT_CHARS,
    MAX_TOOLTIP_TITLE_CHARS,
};

/// Drops control characters and bidi overrides, strips XML/HTML markup tags,
/// safely decodes XML/HTML entities, and clamps the result to `max_chars`.
#[must_use]
pub fn sanitize_text(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let stripped = strip_tags(input);
    let decoded = decode_entities(&stripped);

    let mut filtered = String::with_capacity(decoded.len().min(max_chars));
    let mut count = 0;
    for ch in decoded.chars() {
        if is_bidi_override(ch) || is_forbidden_control(ch) {
            continue;
        }
        filtered.push(ch);
        count += 1;
        if count >= max_chars {
            break;
        }
    }

    filtered
}

/// Sanitizes an item identifier string (`Id`).
#[must_use]
pub fn sanitize_id(id: &str) -> String {
    sanitize_text(id, MAX_ID_CHARS)
}

/// Sanitizes an item title string (`Title`).
#[must_use]
pub fn sanitize_title(title: &str) -> String {
    sanitize_text(title, MAX_TITLE_CHARS)
}

/// Sanitizes a tooltip title string.
#[must_use]
pub fn sanitize_tooltip_title(title: &str) -> String {
    sanitize_text(title, MAX_TOOLTIP_TITLE_CHARS)
}

/// Sanitizes a tooltip body/descriptive text string.
#[must_use]
pub fn sanitize_tooltip_text(text: &str) -> String {
    sanitize_text(text, MAX_TOOLTIP_TEXT_CHARS)
}

/// Validates an icon name according to Freedesktop / `kynex-icons` naming rules.
///
/// An icon name must:
/// - Be non-empty and at most [`MAX_ICON_NAME_CHARS`] characters.
/// - Contain only characters in the set `[a-zA-Z0-9._-]`.
/// - Never be an absolute path (must not start with `/` and must not contain `/` or `\\`).
/// - Never contain path traversal sequences (`..`).
///
/// Returns `Some(sanitized_name)` if valid, or `None` if invalid.
#[must_use]
pub fn sanitize_icon_name(raw_name: &str) -> Option<String> {
    let trimmed = raw_name.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_ICON_NAME_CHARS {
        return None;
    }

    // Absolute paths or path separators are strictly rejected
    if trimmed.starts_with('/') || trimmed.contains('/') || trimmed.contains('\\') {
        return None;
    }

    // Traversal sequences are strictly rejected
    if trimmed.contains("..") {
        return None;
    }

    // Must strictly match [a-zA-Z0-9._-]
    for c in trimmed.chars() {
        if !c.is_ascii_alphanumeric() && c != '.' && c != '_' && c != '-' {
            return None;
        }
    }

    Some(trimmed.to_string())
}

/// Strips all XML/HTML markup tags, including `<script>...</script>` and `<style>...</style>` blocks.
fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            let mut tag_content = String::new();
            for tag_ch in chars.by_ref() {
                if tag_ch == '>' {
                    break;
                }
                tag_content.push(tag_ch);
            }

            let tag_lower = tag_content.trim().to_ascii_lowercase();
            if tag_lower.starts_with("script") || tag_lower.starts_with("style") {
                let close_name = if tag_lower.starts_with("script") {
                    "script"
                } else {
                    "style"
                };
                skip_until_closing_tag(&mut chars, close_name);
            }
        } else {
            out.push(ch);
        }
    }

    out
}

/// Skips characters until a closing tag `</name>` is encountered.
fn skip_until_closing_tag<I>(chars: &mut std::iter::Peekable<I>, name: &str)
where
    I: Iterator<Item = char>,
{
    while let Some(ch) = chars.next() {
        if ch == '<' {
            let mut tag_content = String::new();
            for tag_ch in chars.by_ref() {
                if tag_ch == '>' {
                    break;
                }
                tag_content.push(tag_ch);
            }
            let trimmed = tag_content.trim().to_ascii_lowercase();
            if trimmed.starts_with('/') && trimmed[1..].trim().starts_with(name) {
                break;
            }
        }
    }
}

/// Decodes standard XML and HTML character entities.
fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            let mut found_semicolon = false;

            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next();
                    found_semicolon = true;
                    break;
                }
                if next_ch.is_whitespace() || next_ch == '&' || next_ch == '<' || entity.len() >= 10
                {
                    break;
                }
                if let Some(c) = chars.next() {
                    entity.push(c);
                }
            }

            if found_semicolon {
                match entity.as_str() {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "apos" => out.push('\''),
                    "nbsp" => out.push(' '),
                    _ if entity.starts_with('#') => {
                        if let Some(decoded_char) = decode_numeric_entity(&entity[1..]) {
                            out.push(decoded_char);
                        } else {
                            out.push('&');
                            out.push_str(&entity);
                            out.push(';');
                        }
                    }
                    _ => {
                        out.push('&');
                        out.push_str(&entity);
                        out.push(';');
                    }
                }
            } else {
                out.push('&');
                out.push_str(&entity);
            }
        } else {
            out.push(ch);
        }
    }

    out
}

/// Decodes numeric decimal or hex character entities.
fn decode_numeric_entity(num_str: &str) -> Option<char> {
    let code = if let Some(hex) = num_str
        .strip_prefix('x')
        .or_else(|| num_str.strip_prefix('X'))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        num_str.parse::<u32>().ok()?
    };

    char::from_u32(code)
}

/// Checks whether a character is an ASCII control character (except newline and tab).
fn is_forbidden_control(ch: char) -> bool {
    let u = ch as u32;
    if u == 0x0A || u == 0x09 {
        return false;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_id_and_title() {
        let evil = "<script>alert(1)</script>Hello\u{202E}World\x00\x07";
        let cleaned = sanitize_title(evil);
        assert_eq!(cleaned, "HelloWorld");

        let long_str = "a".repeat(200);
        assert_eq!(sanitize_title(&long_str).len(), MAX_TITLE_CHARS);
        assert_eq!(sanitize_id(&long_str).len(), MAX_ID_CHARS);
        assert_eq!(
            sanitize_tooltip_title(&long_str).len(),
            MAX_TOOLTIP_TITLE_CHARS
        );
        assert_eq!(
            sanitize_tooltip_text(&long_str).len(),
            MAX_TOOLTIP_TEXT_CHARS.min(200)
        );
    }

    #[test]
    fn test_sanitize_icon_name() {
        assert_eq!(
            sanitize_icon_name("audio-volume-high"),
            Some("audio-volume-high".to_string())
        );
        assert_eq!(
            sanitize_icon_name("org.kde.plasma"),
            Some("org.kde.plasma".to_string())
        );
        assert_eq!(sanitize_icon_name("/usr/share/icons/evil.png"), None);
        assert_eq!(sanitize_icon_name("../evil.png"), None);
        assert_eq!(sanitize_icon_name("foo/bar"), None);
        assert_eq!(sanitize_icon_name("foo\\bar"), None);
        assert_eq!(sanitize_icon_name("evil$icon"), None);
        assert_eq!(sanitize_icon_name(""), None);
        assert_eq!(sanitize_icon_name(&"a".repeat(200)), None);
    }

    #[test]
    fn test_entity_decoding() {
        assert_eq!(sanitize_text("&amp;&lt;&gt;&quot;", 50), "&<>\"");
        assert_eq!(sanitize_text("&#65;&#x42;", 50), "AB");
    }
}
