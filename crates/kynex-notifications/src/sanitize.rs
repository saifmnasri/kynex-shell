//! Sanitization, markup stripping, entity decoding, and limit validation.

use crate::error::NotificationError;
use crate::limits::{
    MAX_ACTIONS, MAX_ACTION_KEY_LEN, MAX_ACTION_LABEL_BYTES, MAX_APP_NAME_BYTES, MAX_BODY_BYTES,
    MAX_SUMMARY_BYTES,
};

/// Drops control characters, bidi overrides, strips all XML/HTML markup,
/// safely decodes XML/HTML entities, and clamps the result to `max_bytes`
/// sliced strictly on a valid UTF-8 character boundary.
#[must_use]
pub fn sanitize_text(input: &str, max_bytes: usize) -> String {
    let stripped = strip_tags(input);
    let decoded = decode_entities(&stripped);

    let mut filtered = String::with_capacity(decoded.len());
    for ch in decoded.chars() {
        if is_bidi_override(ch) || is_forbidden_control(ch) {
            continue;
        }
        filtered.push(ch);
    }

    let clamped = truncate_to_char_boundary(&filtered, max_bytes);
    clamped.to_string()
}

/// Sanitizes a notification application name (capped at 64 bytes).
#[must_use]
pub fn sanitize_app_name(app_name: &str) -> String {
    sanitize_text(app_name, MAX_APP_NAME_BYTES)
}

/// Sanitizes a notification summary (capped at 256 bytes).
#[must_use]
pub fn sanitize_summary(summary: &str) -> String {
    sanitize_text(summary, MAX_SUMMARY_BYTES)
}

/// Sanitizes a notification body (capped at 4 KiB / 4096 bytes).
#[must_use]
pub fn sanitize_body(body: &str) -> String {
    sanitize_text(body, MAX_BODY_BYTES)
}

/// Sanitizes an action button label (capped at 64 bytes).
#[must_use]
pub fn sanitize_action_label(label: &str) -> String {
    sanitize_text(label, MAX_ACTION_LABEL_BYTES)
}

/// Validates an action identifier key according to spec and security bounds.
///
/// Keys must be non-empty, at most 32 characters, and contain only
/// characters in the ASCII set `[a-zA-Z0-9._-]`.
///
/// # Errors
///
/// Returns [`NotificationError::InvalidArgs`] or [`NotificationError::LimitsExceeded`]
/// if the key is empty, too long, or contains disallowed characters.
pub fn validate_action_key(key: &str) -> Result<(), NotificationError> {
    if key.is_empty() {
        return Err(NotificationError::InvalidArgs("Action key cannot be empty"));
    }
    if key.len() > MAX_ACTION_KEY_LEN {
        return Err(NotificationError::LimitsExceeded(
            "Action key exceeds 32 characters",
        ));
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err(NotificationError::InvalidArgs(
            "Action key contains invalid characters (allowed: [a-zA-Z0-9._-])",
        ));
    }
    Ok(())
}

/// Validates a list of actions passed as `[key1, label1, key2, label2, ...]`.
///
/// Returns a sanitized `Vec<(key, label)>`, or an error if limits are violated.
///
/// # Errors
///
/// Returns [`NotificationError::InvalidArgs`] or [`NotificationError::LimitsExceeded`]
/// if the action list has odd length, exceeds 8 pairs, or contains invalid keys.
pub fn validate_and_sanitize_actions(
    raw_actions: &[String],
) -> Result<Vec<(String, String)>, NotificationError> {
    if !raw_actions.len().is_multiple_of(2) {
        return Err(NotificationError::InvalidArgs(
            "Actions array length must be even (pairs of key and label)",
        ));
    }
    let pair_count = raw_actions.len() / 2;
    if pair_count > MAX_ACTIONS {
        return Err(NotificationError::LimitsExceeded(
            "Actions count exceeds maximum of 8",
        ));
    }

    let mut actions = Vec::with_capacity(pair_count);
    for chunk in raw_actions.as_chunks::<2>().0 {
        let key = &chunk[0];
        let label = &chunk[1];

        validate_action_key(key)?;
        let sanitized_label = sanitize_action_label(label);
        actions.push((key.clone(), sanitized_label));
    }

    Ok(actions)
}

/// Strips all XML/HTML markup tags, including `<script>...</script>` and `<style>...</style>` blocks.
fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            // Read tag content until '>' or EOF
            let mut tag_content = String::new();
            for tag_ch in chars.by_ref() {
                if tag_ch == '>' {
                    break;
                }
                tag_content.push(tag_ch);
            }

            let tag_lower = tag_content.trim().to_ascii_lowercase();

            // If this is an opening <script> or <style> tag, drop everything until the closing tag
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

/// Skips characters until a closing tag `</name>` is found.
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

/// Decodes standard XML and HTML numeric character entities.
fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            let mut found_semicolon = false;

            // Look ahead for semicolon up to 10 characters
            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next(); // consume ';'
                    found_semicolon = true;
                    break;
                }
                if next_ch.is_whitespace() || next_ch == '&' || next_ch == '<' || entity.len() >= 10
                {
                    break;
                }
                entity.push(chars.next().unwrap_or(' '));
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
                            // If invalid numeric entity, emit verbatim
                            out.push('&');
                            out.push_str(&entity);
                            out.push(';');
                        }
                    }
                    _ => {
                        // Unknown named entity: emit verbatim
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

/// Decodes numeric decimal (`123`) or hex (`x7B`) entities.
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

/// Checks whether a character is an ASCII control character (except `\n` and `\t`).
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

/// Slices a string to at most `max_bytes` without splitting a multi-byte UTF-8 character.
#[must_use]
pub fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}
