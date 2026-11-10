//! Parser and data model for Desktop Entry Specification files (`.desktop`).
//!
//! Complies with the Freedesktop.org Desktop Entry Specification (Version 1.5).
//! All section citations reference the Desktop Entry Specification.

use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// Maximum allowable size for a `.desktop` file (64 KiB).
pub const MAX_FILE_SIZE: u64 = 64 * 1024;

/// Maximum allowable length for a single line in a `.desktop` file (4 KiB).
pub const MAX_LINE_LENGTH: usize = 4096;

/// Maximum number of groups allowed in a single desktop entry file.
pub const MAX_GROUPS: usize = 50;

/// Maximum number of key-value pairs allowed per group.
pub const MAX_KEYS_PER_GROUP: usize = 200;

/// Errors that can occur during desktop entry parsing or validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DesktopEntryError {
    /// File size exceeds the hard limit of 64 KiB.
    #[error("File size {0} bytes exceeds maximum allowed limit of {MAX_FILE_SIZE} bytes")]
    FileTooLarge(u64),

    /// A line exceeds the hard limit of 4 KiB.
    #[error("Line length {0} bytes exceeds maximum allowed limit of {MAX_LINE_LENGTH} bytes")]
    LineTooLong(usize),

    /// Total groups count exceeds maximum limit.
    #[error("Group count {0} exceeds maximum allowed limit of {MAX_GROUPS}")]
    TooManyGroups(usize),

    /// Key count in a group exceeds maximum limit.
    #[error("Group '{0}' key count {1} exceeds maximum allowed limit of {MAX_KEYS_PER_GROUP}")]
    TooManyKeys(String, usize),

    /// File is not a regular file (e.g. FIFO, socket, device).
    #[error("Path '{0}' is not a regular file")]
    NotARegularFile(String),

    /// Missing mandatory `[Desktop Entry]` group.
    /// Desktop Entry Specification Section "Basic format of the file".
    #[error("Missing mandatory [Desktop Entry] group header")]
    MissingDesktopEntryGroup,

    /// Type is not Application.
    /// Desktop Entry Specification Section "Recognized desktop entry keys": Type=Application.
    #[error("Entry Type '{0}' is not 'Application'")]
    NotAnApplication(String),

    /// Missing mandatory Name key.
    /// Desktop Entry Specification Section "Recognized desktop entry keys": Name.
    #[error("Missing mandatory 'Name' key")]
    MissingName,

    /// Entry is marked Hidden=true.
    /// Desktop Entry Specification Section "Recognized desktop entry keys": `Hidden`.
    #[error("Entry is hidden")]
    Hidden,

    /// Entry is marked NoDisplay=true.
    /// Desktop Entry Specification Section "Recognized desktop entry keys": `NoDisplay`.
    #[error("Entry is marked NoDisplay")]
    NoDisplay,

    /// Entry is excluded by `OnlyShowIn` or `NotShowIn`.
    /// Desktop Entry Specification Section "Recognized desktop entry keys": `OnlyShowIn` / `NotShowIn`.
    #[error("Entry is not allowed in current desktop '{0}'")]
    DesktopNotAllowed(String),

    /// `TryExec` binary was not found or is not executable.
    /// Desktop Entry Specification Section "Recognized desktop entry keys": `TryExec`.
    #[error("TryExec binary '{0}' is not executable or not found")]
    TryExecNotFound(String),

    /// Exec key is missing or invalid.
    #[error("Missing or invalid 'Exec' key: {0}")]
    InvalidExec(String),

    /// General I/O or syntax error.
    #[error("I/O error: {0}")]
    Io(String),

    /// Malformed group or key format.
    #[error("Malformed desktop entry syntax: {0}")]
    SyntaxError(String),
}

/// A localized string holding a default value and language-specific overrides.
/// Desktop Entry Specification Section "Localized values for keys".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalizedString {
    /// Default (unlocalized) value.
    pub default: String,
    /// Localized values keyed by locale tag, e.g. "`fr_FR`", "fr", "`de@euro`".
    pub localized: HashMap<String, String>,
}

impl LocalizedString {
    /// Creates a new `LocalizedString` with a default value.
    #[must_use]
    pub fn new(default: impl Into<String>) -> Self {
        Self {
            default: default.into(),
            localized: HashMap::new(),
        }
    }

    /// Resolves the localized value using standard locale precedence.
    ///
    /// Desktop Entry Specification Section "Localized values for keys":
    /// Matching order for `lang_COUNTRY.ENCODING@MODIFIER`:
    /// 1. `lang_COUNTRY@MODIFIER`
    /// 2. `lang_COUNTRY`
    /// 3. `lang@MODIFIER`
    /// 4. `lang`
    /// 5. default
    #[must_use]
    pub fn resolve(&self, locale: Option<&str>) -> &str {
        let Some(loc) = locale else {
            return &self.default;
        };

        for candidate in generate_locale_candidates(loc) {
            if let Some(val) = self.localized.get(&candidate) {
                return val;
            }
        }

        &self.default
    }
}

/// Generates locale candidate tags in priority order from a POSIX locale string.
/// Desktop Entry Specification Section "Localized values for keys".
#[must_use]
pub fn generate_locale_candidates(locale: &str) -> Vec<String> {
    // Strip encoding (e.g. ".UTF-8")
    let without_encoding = match locale.split_once('.') {
        Some((prefix, rest)) => match rest.split_once('@') {
            Some((_encoding, modifier)) => format!("{prefix}@{modifier}"),
            None => prefix.to_string(),
        },
        None => locale.to_string(),
    };

    let (base, modifier) = match without_encoding.split_once('@') {
        Some((b, m)) => (b, Some(m)),
        None => (without_encoding.as_str(), None),
    };

    let (lang, country) = match base.split_once('_') {
        Some((l, c)) => (l, Some(c)),
        None => (base, None),
    };

    let mut candidates = Vec::with_capacity(4);

    if let (Some(c), Some(m)) = (country, modifier) {
        candidates.push(format!("{lang}_{c}@{m}"));
    }
    if let Some(c) = country {
        candidates.push(format!("{lang}_{c}"));
    }
    if let Some(m) = modifier {
        candidates.push(format!("{lang}@{m}"));
    }
    candidates.push(lang.to_string());

    candidates
}

/// Decodes value escapes according to Desktop Entry Specification Section "Values".
///
/// Recognized escape sequences:
/// - `\s` -> space (' ')
/// - `\n` -> line feed ('\n')
/// - `\t` -> horizontal tab ('\t')
/// - `\r` -> carriage return ('\r')
/// - `\\` -> backslash ('\')
/// - `\;` -> semicolon (';')
#[must_use]
pub fn decode_escapes(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(';') => out.push(';'),
                Some(other) => {
                    // Unknown escape: keep literal character
                    out.push(other);
                }
                None => {
                    out.push('\\');
                }
            }
        } else {
            out.push(ch);
        }
    }

    out
}

/// Parses a semicolon-separated list of strings according to Desktop Entry Specification Section "Values".
#[must_use]
pub fn parse_string_list(raw: &str) -> Vec<String> {
    let mut list = Vec::new();
    let mut current = String::new();
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next();
                    current.push(';');
                    continue;
                }
            }
            current.push('\\');
        } else if ch == ';' {
            let item = decode_escapes(current.trim());
            if !item.is_empty() {
                list.push(item);
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }

    let remainder = decode_escapes(current.trim());
    if !remainder.is_empty() {
        list.push(remainder);
    }

    list
}

/// Parsed `.desktop` entry containing normalized metadata from `[Desktop Entry]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    /// Desktop file unique ID (e.g. "org.kde.konsole.desktop").
    pub id: String,
    /// Absolute path to the desktop file on disk.
    pub path: String,
    /// Localized name of the application.
    pub name: LocalizedString,
    /// Localized generic name (e.g. "Web Browser").
    pub generic_name: LocalizedString,
    /// Localized comment / tooltip description.
    pub comment: LocalizedString,
    /// Icon name or path.
    pub icon: String,
    /// Raw Exec command line from desktop file.
    pub exec_raw: String,
    /// `TryExec` executable test.
    pub try_exec: Option<String>,
    /// Categories list.
    pub categories: Vec<String>,
    /// Search keywords list.
    pub keywords: Vec<String>,
    /// Desktop environments in which the app must be shown.
    pub only_show_in: Vec<String>,
    /// Desktop environments in which the app must NOT be shown.
    pub not_show_in: Vec<String>,
    /// Whether app is hidden.
    pub hidden: bool,
    /// Whether app is excluded from menus and launchers.
    pub no_display: bool,
    /// Whether application requires running inside a terminal window.
    pub terminal: bool,
    /// `StartupWMClass` for window management grouping and matching.
    pub startup_wm_class: Option<String>,
}

impl DesktopEntry {
    /// Parses desktop entry content from a raw byte slice with security bounds enforcement.
    ///
    /// # Errors
    /// Returns `DesktopEntryError` if bounds are exceeded, required groups/keys are missing,
    /// or entry fails desktop visibility constraints.
    #[allow(clippy::too_many_lines)]
    pub fn parse_bytes(
        id: &str,
        path: &Path,
        bytes: &[u8],
        current_desktop: &str,
    ) -> Result<Self, DesktopEntryError> {
        if bytes.len() as u64 > MAX_FILE_SIZE {
            return Err(DesktopEntryError::FileTooLarge(bytes.len() as u64));
        }

        let content = String::from_utf8_lossy(bytes);
        let mut groups: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current_group: Option<String> = None;

        for line in content.lines() {
            if line.len() > MAX_LINE_LENGTH {
                return Err(DesktopEntryError::LineTooLong(line.len()));
            }

            let trimmed = line.trim();
            // Desktop Entry Specification Section "Basic format of the file":
            // Empty lines and lines starting with # are comments.
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Desktop Entry Specification Section "Group headers":
            // Group header format: [group_name]
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                let group_name = trimmed[1..trimmed.len() - 1].trim().to_string();
                if groups.len() >= MAX_GROUPS && !groups.contains_key(&group_name) {
                    return Err(DesktopEntryError::TooManyGroups(groups.len() + 1));
                }
                groups.entry(group_name.clone()).or_default();
                current_group = Some(group_name);
                continue;
            }

            // Desktop Entry Specification Section "Entries": Key=Value
            if let Some(ref group_name) = current_group {
                if let Some((key_raw, val_raw)) = trimmed.split_once('=') {
                    let key = key_raw.trim().to_string();
                    let val = val_raw.trim().to_string();

                    let group_map = groups.entry(group_name.clone()).or_default();
                    if group_map.len() >= MAX_KEYS_PER_GROUP && !group_map.contains_key(&key) {
                        return Err(DesktopEntryError::TooManyKeys(
                            group_name.clone(),
                            group_map.len() + 1,
                        ));
                    }
                    group_map.insert(key, val);
                }
            }
        }

        // Desktop Entry Specification Section "Basic format of the file":
        // Must contain [Desktop Entry]
        let desktop_group = groups
            .remove("Desktop Entry")
            .ok_or(DesktopEntryError::MissingDesktopEntryGroup)?;

        // Desktop Entry Specification Section "Recognized desktop entry keys":
        // Type=Application is required.
        let entry_type = desktop_group.get("Type").cloned().unwrap_or_default();
        if entry_type != "Application" {
            return Err(DesktopEntryError::NotAnApplication(entry_type));
        }

        let hidden = desktop_group
            .get("Hidden")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        if hidden {
            return Err(DesktopEntryError::Hidden);
        }

        let no_display = desktop_group
            .get("NoDisplay")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        if no_display {
            return Err(DesktopEntryError::NoDisplay);
        }

        let terminal = desktop_group
            .get("Terminal")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));

        let only_show_in = desktop_group
            .get("OnlyShowIn")
            .map(|s| parse_string_list(s))
            .unwrap_or_default();

        let not_show_in = desktop_group
            .get("NotShowIn")
            .map(|s| parse_string_list(s))
            .unwrap_or_default();

        // Check desktop environment filters
        if !only_show_in.is_empty()
            && !only_show_in
                .iter()
                .any(|d| d.eq_ignore_ascii_case(current_desktop))
        {
            return Err(DesktopEntryError::DesktopNotAllowed(
                current_desktop.to_string(),
            ));
        }

        if not_show_in
            .iter()
            .any(|d| d.eq_ignore_ascii_case(current_desktop))
        {
            return Err(DesktopEntryError::DesktopNotAllowed(
                current_desktop.to_string(),
            ));
        }

        let try_exec = desktop_group.get("TryExec").cloned();
        if let Some(ref binary) = try_exec {
            if !is_executable_in_path(binary) {
                return Err(DesktopEntryError::TryExecNotFound(binary.clone()));
            }
        }

        let exec_raw = desktop_group
            .get("Exec")
            .cloned()
            .ok_or_else(|| DesktopEntryError::InvalidExec("Exec key is missing".to_string()))?;

        // Extract localized Name, GenericName, Comment, Keywords
        let mut name = LocalizedString::default();
        let mut generic_name = LocalizedString::default();
        let mut comment = LocalizedString::default();
        let mut keywords = Vec::new();

        for (k, v) in &desktop_group {
            if k == "Name" {
                name.default = decode_escapes(v);
            } else if let Some(loc) = extract_locale_key(k, "Name") {
                name.localized.insert(loc, decode_escapes(v));
            } else if k == "GenericName" {
                generic_name.default = decode_escapes(v);
            } else if let Some(loc) = extract_locale_key(k, "GenericName") {
                generic_name.localized.insert(loc, decode_escapes(v));
            } else if k == "Comment" {
                comment.default = decode_escapes(v);
            } else if let Some(loc) = extract_locale_key(k, "Comment") {
                comment.localized.insert(loc, decode_escapes(v));
            } else if k == "Keywords" {
                keywords = parse_string_list(v);
            }
        }

        if name.default.is_empty() && name.localized.is_empty() {
            return Err(DesktopEntryError::MissingName);
        }

        let icon = desktop_group
            .get("Icon")
            .map(|s| decode_escapes(s))
            .unwrap_or_default();

        let categories = desktop_group
            .get("Categories")
            .map(|s| parse_string_list(s))
            .unwrap_or_default();

        let startup_wm_class = desktop_group
            .get("StartupWMClass")
            .map(|s| decode_escapes(s).trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(Self {
            id: id.to_string(),
            path: path.to_string_lossy().to_string(),
            name,
            generic_name,
            comment,
            icon,
            exec_raw,
            try_exec,
            categories,
            keywords,
            only_show_in,
            not_show_in,
            hidden,
            no_display,
            terminal,
            startup_wm_class,
        })
    }
}

/// Checks if key matches `BaseKey[locale]` and extracts `locale`.
fn extract_locale_key(key: &str, base: &str) -> Option<String> {
    if key.starts_with(base) && key.ends_with(']') {
        let remainder = &key[base.len()..];
        if remainder.starts_with('[') {
            return Some(remainder[1..remainder.len() - 1].to_string());
        }
    }
    None
}

/// Checks if an executable exists and is executable in `$PATH` or at an absolute/relative path.
#[must_use]
pub fn is_executable_in_path(binary: &str) -> bool {
    let p = Path::new(binary);
    if binary.contains('/') {
        return is_file_executable(p);
    }

    if let Ok(paths) = std::env::var("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(binary);
            if is_file_executable(&candidate) {
                return true;
            }
        }
    }

    false
}

#[cfg(unix)]
fn is_file_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = path.metadata() {
        if meta.is_file() {
            return meta.permissions().mode() & 0o111 != 0;
        }
    }
    false
}

#[cfg(not(unix))]
fn is_file_executable(path: &Path) -> bool {
    path.is_file()
}
