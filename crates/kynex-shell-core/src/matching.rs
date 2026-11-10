//! Window to application matching logic.
//!
//! Provides deterministic matching from compositor window `app_id` strings to indexed
//! desktop applications adhering to:
//! 1. `StartupWMClass` matching (exact and case-folded).
//! 2. Desktop ID matching (with `.desktop` suffix stripping, exact and case-folded).
//! 3. Reverse-DNS ID matching (matching trailing reverse-DNS component, e.g. `org.gnome.Nautilus` vs `nautilus`).
//! 4. Hyphenated prefix folding (e.g. `gnome-text-editor` vs `TextEditor`).

/// Representation of an application candidate for window matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCandidate {
    /// Desktop entry unique identifier (e.g. `org.gnome.Nautilus.desktop`).
    pub id: String,
    /// Explicit `StartupWMClass` from desktop entry if specified.
    pub startup_wm_class: Option<String>,
    /// Icon name or path associated with this application.
    pub icon_name: String,
}

impl AppCandidate {
    /// Create a new application candidate.
    pub fn new(
        id: impl Into<String>,
        startup_wm_class: Option<impl Into<String>>,
        icon_name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            startup_wm_class: startup_wm_class.map(Into::into),
            icon_name: icon_name.into(),
        }
    }
}

/// Strip `.desktop` suffix from an ID or class name if present.
#[must_use]
pub fn strip_desktop_suffix(raw: &str) -> &str {
    raw.strip_suffix(".desktop").unwrap_or(raw)
}

/// Extract the last segment of a reverse-DNS identifier (e.g. `"org.gnome.Nautilus"` -> `"Nautilus"`).
#[must_use]
pub fn extract_reverse_dns_leaf(raw: &str) -> &str {
    let clean = strip_desktop_suffix(raw);
    clean.rsplit('.').next().unwrap_or(clean)
}

/// Normalize an identifier for case-folded, hyphen-insensitive comparison.
#[must_use]
pub fn normalize_for_comparison(raw: &str) -> String {
    let clean = strip_desktop_suffix(raw);
    clean
        .chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Match a running window's `app_id` to the best matching application candidate.
///
/// Precedence order:
/// 1. Exact match on `StartupWMClass`.
/// 2. Case-folded match on `StartupWMClass`.
/// 3. Exact match on full desktop ID (ignoring `.desktop` suffix).
/// 4. Case-folded match on full desktop ID (ignoring `.desktop` suffix).
/// 5. Exact match on reverse-DNS leaf segment (e.g. `org.gnome.Terminal` -> `Terminal`).
/// 6. Case-folded match on reverse-DNS leaf segment.
/// 7. Normalized match (stripping hyphens and underscores, e.g. `gnome-text-editor` vs `TextEditor`).
#[must_use]
pub fn match_window_to_app<'a>(
    window_app_id: &str,
    candidates: &'a [AppCandidate],
) -> Option<&'a AppCandidate> {
    let raw_app_id = window_app_id.trim();
    if raw_app_id.is_empty() {
        return None;
    }

    let clean_app_id = strip_desktop_suffix(raw_app_id);
    let app_id_leaf = extract_reverse_dns_leaf(clean_app_id);
    let normalized_app_id = normalize_for_comparison(clean_app_id);

    // 1. Exact StartupWMClass
    for candidate in candidates {
        if let Some(ref wm_class) = candidate.startup_wm_class {
            if wm_class == clean_app_id || wm_class == raw_app_id {
                return Some(candidate);
            }
        }
    }

    // 2. Case-folded StartupWMClass
    for candidate in candidates {
        if let Some(ref wm_class) = candidate.startup_wm_class {
            if wm_class.eq_ignore_ascii_case(clean_app_id) {
                return Some(candidate);
            }
        }
    }

    // 3. Exact full desktop ID match
    for candidate in candidates {
        let clean_id = strip_desktop_suffix(&candidate.id);
        if clean_id == clean_app_id || candidate.id == raw_app_id {
            return Some(candidate);
        }
    }

    // 4. Case-folded full desktop ID match
    for candidate in candidates {
        let clean_id = strip_desktop_suffix(&candidate.id);
        if clean_id.eq_ignore_ascii_case(clean_app_id) {
            return Some(candidate);
        }
    }

    // 5. Exact reverse-DNS leaf match (both directions)
    for candidate in candidates {
        let clean_id = strip_desktop_suffix(&candidate.id);
        let cand_leaf = extract_reverse_dns_leaf(clean_id);
        if cand_leaf == app_id_leaf || cand_leaf == clean_app_id || clean_id == app_id_leaf {
            return Some(candidate);
        }
    }

    // 6. Case-folded reverse-DNS leaf match
    for candidate in candidates {
        let clean_id = strip_desktop_suffix(&candidate.id);
        let cand_leaf = extract_reverse_dns_leaf(clean_id);
        if cand_leaf.eq_ignore_ascii_case(app_id_leaf)
            || cand_leaf.eq_ignore_ascii_case(clean_app_id)
            || clean_id.eq_ignore_ascii_case(app_id_leaf)
        {
            return Some(candidate);
        }
    }

    // 7. Normalized match (hyphens/underscores/case stripped)
    for candidate in candidates {
        let clean_id = strip_desktop_suffix(&candidate.id);
        let cand_norm = normalize_for_comparison(clean_id);
        let cand_leaf_norm = normalize_for_comparison(extract_reverse_dns_leaf(clean_id));

        if cand_norm == normalized_app_id
            || cand_leaf_norm == normalized_app_id
            || normalized_app_id.ends_with(&cand_leaf_norm)
            || cand_norm.ends_with(&normalized_app_id)
        {
            return Some(candidate);
        }
    }

    None
}
