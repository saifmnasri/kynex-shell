//! In-house ranked search matcher over application metadata (< 200 lines).
//!
//! Evaluates matches across Name, `GenericName`, Keywords, and Categories using
//! exact, prefix, word-start, and subsequence boosts with deterministic stable ordering.

use crate::entry::DesktopEntry;

/// A ranked search match result referencing a `DesktopEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// Desktop entry unique ID.
    pub id: String,
    /// Calculated relevance score (higher is better).
    pub score: u32,
}

/// Matches a single field against a lowercase query, returning a score.
#[allow(clippy::cast_possible_truncation)]
fn match_field(text: &str, query: &str) -> u32 {
    let lower = text.to_lowercase();
    if lower == query {
        return 1000;
    }
    if lower.starts_with(query) {
        return 600 + (100 / (1 + (lower.len() - query.len()) as u32));
    }

    // Word start match (words separated by whitespace, hyphens, dots, underscores)
    let is_word_start = lower
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '.')
        .any(|w| w.starts_with(query));
    if is_word_start {
        return 400;
    }

    // Subsequence match: query chars appear in order
    let mut query_chars = query.chars();
    let mut current_target = query_chars.next();
    let mut match_span_start = None;
    let mut match_span_end = 0;

    for (idx, ch) in lower.chars().enumerate() {
        if let Some(target) = current_target {
            if ch == target {
                if match_span_start.is_none() {
                    match_span_start = Some(idx);
                }
                match_span_end = idx;
                current_target = query_chars.next();
            }
        }
    }

    if current_target.is_none() {
        if let Some(start) = match_span_start {
            let span = (match_span_end - start + 1) as u32;
            let compactness = 100 / (1 + (span.saturating_sub(query.len() as u32)));
            return 100 + compactness;
        }
    }

    0
}

/// Computes the overall match score for a `DesktopEntry` given a query and active locale.
fn score_entry(entry: &DesktopEntry, query: &str, locale: Option<&str>) -> (u32, u32) {
    let name = entry.name.resolve(locale);
    let name_score = match_field(name, query);

    let generic_score = match_field(entry.generic_name.resolve(locale), query);

    let mut keyword_score = 0;
    for kw in &entry.keywords {
        let s = match_field(kw, query);
        if s > keyword_score {
            keyword_score = s;
        }
    }

    let mut category_score = 0;
    for cat in &entry.categories {
        let s = match_field(cat, query);
        if s > category_score {
            category_score = s;
        }
    }

    // Weighted combination:
    // Name (weight 5), GenericName (weight 3), Keywords (weight 2), Categories (weight 1)
    let total = (name_score * 5) + (generic_score * 3) + (keyword_score * 2) + category_score;
    (total, name_score)
}

/// Searches and ranks entries matching `query` up to `limit` entries.
///
/// Returns stable results deterministically sorted by score, name score, length, and id.
#[must_use]
pub fn search_entries<'a>(
    entries: &'a [DesktopEntry],
    query: &str,
    limit: usize,
    locale: Option<&str>,
) -> Vec<&'a DesktopEntry> {
    let q = query.trim().to_lowercase();
    let cap = if limit == 0 { 20 } else { limit.min(50) };

    if q.is_empty() {
        let mut sorted: Vec<&'a DesktopEntry> = entries.iter().collect();
        sorted.sort_by(|a, b| {
            a.name
                .resolve(locale)
                .cmp(b.name.resolve(locale))
                .then_with(|| a.id.cmp(&b.id))
        });
        sorted.truncate(cap);
        return sorted;
    }

    let mut scored: Vec<(&'a DesktopEntry, u32, u32)> = Vec::new();

    for entry in entries {
        let (total_score, name_score) = score_entry(entry, &q, locale);
        if total_score > 0 {
            scored.push((entry, total_score, name_score));
        }
    }

    scored.sort_by(|(entry_a, score_a, name_a), (entry_b, score_b, name_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| name_b.cmp(name_a))
            .then_with(|| {
                entry_a
                    .name
                    .resolve(locale)
                    .len()
                    .cmp(&entry_b.name.resolve(locale).len())
            })
            .then_with(|| {
                entry_a
                    .name
                    .resolve(locale)
                    .cmp(entry_b.name.resolve(locale))
            })
            .then_with(|| entry_a.id.cmp(&entry_b.id))
    });

    scored
        .into_iter()
        .take(cap)
        .map(|(entry, _, _)| entry)
        .collect()
}
