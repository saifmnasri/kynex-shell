//! Exec key parser and field code expander according to Desktop Entry Specification.
//!
//! Citations: Desktop Entry Specification (Version 1.5), Section "The Exec key".
//! Produces an `argv: Vec<String>` array for direct execution without a shell.

use thiserror::Error;

/// Contextual metadata provided to expand Exec field codes.
#[derive(Debug, Clone, Default)]
pub struct ExecContext<'a> {
    /// Translated application Name (%c).
    pub name: &'a str,
    /// Icon key string (%i).
    pub icon: &'a str,
    /// Path to the desktop file (%k).
    pub file_path: &'a str,
    /// Target file paths (%f, %F).
    pub files: &'a [&'a str],
    /// Target URLs (%u, %U).
    pub urls: &'a [&'a str],
}

/// Errors during Exec key parsing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecError {
    /// Exec line is empty.
    #[error("Exec string is empty")]
    Empty,

    /// Unclosed double quote string.
    #[error("Unclosed double quote in Exec command line")]
    UnclosedQuote,

    /// Executable binary is missing.
    #[error("Missing executable binary name in Exec command line")]
    MissingExecutable,
}

/// Parses an Exec command line into an argument vector (`argv`) without shell interpretation.
///
/// Desktop Entry Specification Section "The Exec key":
/// - Arguments may be enclosed in double quotes `"...""`.
/// - Within double quotes, the characters `\"`, `\\`, `\$`, and `` \` `` are escaped with backslash.
/// - Field codes (%f, %F, %u, %U, %i, %c, %k, %%) are expanded.
/// - Deprecated field codes (%d, %D, %n, %N, %v, %m) are removed.
///
/// # Errors
/// Returns `ExecError` if quotes are unclosed or Exec line is empty.
#[allow(clippy::too_many_lines)]
pub fn parse_exec(exec_line: &str, ctx: &ExecContext<'_>) -> Result<Vec<String>, ExecError> {
    let trimmed = exec_line.trim();
    if trimmed.is_empty() {
        return Err(ExecError::Empty);
    }

    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = trimmed.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                in_quotes = false;
            } else if ch == '\\' {
                // Inside double quotes: \", \\, \$, \`
                match chars.peek() {
                    Some('"' | '\\' | '$' | '`') => {
                        if let Some(next_ch) = chars.next() {
                            current.push(next_ch);
                        }
                    }
                    _ => {
                        // Keep backslash literally
                        current.push('\\');
                    }
                }
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == '\\' {
            // Outside quotes: backslash escapes the next character
            if let Some(next_ch) = chars.next() {
                current.push(next_ch);
            }
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(current);
                current = String::new();
            }
        } else {
            current.push(ch);
        }
    }

    if in_quotes {
        return Err(ExecError::UnclosedQuote);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    if tokens.is_empty() {
        return Err(ExecError::MissingExecutable);
    }

    // Now expand field codes across tokens
    let mut argv: Vec<String> = Vec::with_capacity(tokens.len());

    for (idx, token) in tokens.into_iter().enumerate() {
        // Binary name (token 0) is never expanded with file codes, but can have %%
        if idx == 0 {
            let binary = expand_token_field_codes(&token, ctx, true);
            if binary.is_empty() {
                return Err(ExecError::MissingExecutable);
            }
            argv.push(binary[0].clone());
            continue;
        }

        let expanded_args = expand_token_field_codes(&token, ctx, false);
        argv.extend(expanded_args);
    }

    if argv.is_empty() || argv[0].is_empty() {
        return Err(ExecError::MissingExecutable);
    }

    Ok(argv)
}

/// Expands field codes in a single token according to Desktop Entry Specification Section "The Exec key".
fn expand_token_field_codes(token: &str, ctx: &ExecContext<'_>, is_binary: bool) -> Vec<String> {
    if !token.contains('%') {
        return vec![token.to_string()];
    }

    let mut out = String::new();
    let mut chars = token.chars().peekable();
    let mut multi_args: Option<Vec<String>> = None;

    while let Some(ch) = chars.next() {
        if ch == '%' {
            match chars.next() {
                Some('%') => {
                    // Literal '%'
                    out.push('%');
                }
                Some('f') => {
                    // %f: single file name
                    if !is_binary {
                        if let Some(&first) = ctx.files.first() {
                            out.push_str(first);
                        }
                    }
                }
                Some('F') => {
                    // %F: list of files
                    if !is_binary {
                        if ctx.files.len() > 1 {
                            // Expands into multiple separate arguments
                            multi_args = Some(ctx.files.iter().map(|&s| s.to_string()).collect());
                        } else if let Some(&first) = ctx.files.first() {
                            out.push_str(first);
                        }
                    }
                }
                Some('u') => {
                    // %u: single URL
                    if !is_binary {
                        if let Some(&first) = ctx.urls.first() {
                            out.push_str(first);
                        }
                    }
                }
                Some('U') => {
                    // %U: list of URLs
                    if !is_binary {
                        if ctx.urls.len() > 1 {
                            multi_args = Some(ctx.urls.iter().map(|&s| s.to_string()).collect());
                        } else if let Some(&first) = ctx.urls.first() {
                            out.push_str(first);
                        }
                    }
                }
                Some('i') => {
                    // %i: "--icon <icon>" if icon non-empty, otherwise nothing
                    if !is_binary && !ctx.icon.is_empty() {
                        multi_args = Some(vec!["--icon".to_string(), ctx.icon.to_string()]);
                    }
                }
                Some('c') => {
                    // %c: translated name
                    if !is_binary {
                        out.push_str(ctx.name);
                    }
                }
                Some('k') => {
                    // %k: path of desktop file
                    if !is_binary {
                        out.push_str(ctx.file_path);
                    }
                }
                // Deprecated, unknown, or trailing % field codes: dropped
                Some(_) | None => {}
            }
        } else {
            out.push(ch);
        }
    }

    if let Some(args) = multi_args {
        args
    } else if out.is_empty() {
        // If the token consisted solely of dropped/empty field codes (like %f when no file), omit argument
        Vec::new()
    } else {
        vec![out]
    }
}
