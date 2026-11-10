//! Benchmark utility for icon lookup latency (cold vs warm).
//!
//! Evaluates Freedesktop Icon Theme Spec v0.13 lookup performance across
//! real desktop icon names found on the host system.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::manual_let_else,
    clippy::print_stdout
)]

use std::fs;
use std::sync::Arc;
use std::time::Instant;

use kynex_icons::cache::IconCache;
use kynex_icons::config::resolve_active_theme;
use kynex_icons::lookup::IconResolver;
use kynex_icons::roots::IconRoots;

fn collect_installed_icon_names() -> Vec<String> {
    let mut names = Vec::new();
    let app_dirs = ["/usr/share/applications", "/usr/local/share/applications"];

    for dir in &app_dirs {
        let read_dir = match fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                if let Ok(content) = fs::read_to_string(&path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if let Some(val) = trimmed.strip_prefix("Icon=") {
                            let val = val.trim();
                            if !val.is_empty()
                                && !val.contains('/')
                                && !names.contains(&val.to_string())
                            {
                                names.push(val.to_string());
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    if names.is_empty() {
        names = vec![
            "utilities-terminal".to_string(),
            "org.gnome.Calculator".to_string(),
            "system-file-manager".to_string(),
            "text-editor".to_string(),
            "preferences-system".to_string(),
        ];
    }
    names
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((pct / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn main() {
    println!("=== Kynex Icons Lookup Benchmark (Freedesktop Spec v0.13) ===");
    let roots = IconRoots::from_env();
    let active_theme = resolve_active_theme(&roots);
    println!("Active icon theme: {active_theme}");
    println!("Icon roots: {} directories", roots.theme_roots().len());

    let icon_names = collect_installed_icon_names();
    println!(
        "Benchmarking {} installed application icons...",
        icon_names.len()
    );

    let size = 24;
    let scale = 1;

    // 1. Cold Lookups (uncached)
    let mut cold_durations_us = Vec::with_capacity(icon_names.len());
    let mut cold_resolved = 0;
    for icon in &icon_names {
        // Create fresh resolver with fresh empty cache for each cold lookup
        let cache = Arc::new(IconCache::new(1024));
        let resolver = IconResolver::with_cache(roots.clone(), cache);

        let t0 = Instant::now();
        let res = resolver.find_icon(icon, size, scale, &active_theme);
        let elapsed = t0.elapsed().as_micros() as f64;
        cold_durations_us.push(elapsed);
        if let Ok(Some(_)) = res {
            cold_resolved += 1;
        }
    }

    cold_durations_us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let cold_p50 = percentile(&cold_durations_us, 50.0);
    let cold_p90 = percentile(&cold_durations_us, 90.0);
    let cold_p95 = percentile(&cold_durations_us, 95.0);
    let cold_p99 = percentile(&cold_durations_us, 99.0);

    // 2. Warm Lookups (cached)
    let warm_cache = Arc::new(IconCache::new(1024));
    let warm_resolver = IconResolver::with_cache(roots.clone(), warm_cache);

    // Populate cache first
    for icon in &icon_names {
        let _ = warm_resolver.find_icon(icon, size, scale, &active_theme);
    }

    let mut warm_durations_us = Vec::with_capacity(icon_names.len());
    let mut warm_resolved = 0;
    for icon in &icon_names {
        let t0 = Instant::now();
        let res = warm_resolver.find_icon(icon, size, scale, &active_theme);
        let elapsed = t0.elapsed().as_micros() as f64;
        warm_durations_us.push(elapsed);
        if let Ok(Some(_)) = res {
            warm_resolved += 1;
        }
    }

    warm_durations_us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let warm_p50 = percentile(&warm_durations_us, 50.0);
    let warm_p90 = percentile(&warm_durations_us, 90.0);
    let warm_p95 = percentile(&warm_durations_us, 95.0);
    let warm_p99 = percentile(&warm_durations_us, 99.0);

    println!("\n--- Results Summary ---");
    println!("Total icons sampled: {}", icon_names.len());
    println!("Resolved successfully: cold={cold_resolved}, warm={warm_resolved}");
    println!(
        "Cold Lookup: p50 = {:.2} µs ({:.4} ms) | p90 = {:.2} µs | p95 = {:.2} µs ({:.4} ms) | p99 = {:.2} µs",
        cold_p50, cold_p50 / 1000.0, cold_p90, cold_p95, cold_p95 / 1000.0, cold_p99
    );
    println!(
        "Warm Lookup: p50 = {:.2} µs ({:.4} ms) | p90 = {:.2} µs | p95 = {:.2} µs ({:.4} ms) | p99 = {:.2} µs",
        warm_p50, warm_p50 / 1000.0, warm_p90, warm_p95, warm_p95 / 1000.0, warm_p99
    );

    // Output JSON for budget verification
    let json_content = format!(
        r#"{{
  "active_theme": "{}",
  "sample_count": {},
  "cold": {{
    "p50_us": {:.2},
    "p95_us": {:.2},
    "p50_ms": {:.4},
    "p95_ms": {:.4}
  }},
  "warm": {{
    "p50_us": {:.2},
    "p95_us": {:.2},
    "p50_ms": {:.4},
    "p95_ms": {:.4}
  }}
}}
"#,
        active_theme,
        icon_names.len(),
        cold_p50,
        cold_p95,
        cold_p50 / 1000.0,
        cold_p95 / 1000.0,
        warm_p50,
        warm_p95,
        warm_p50 / 1000.0,
        warm_p95 / 1000.0
    );

    let _ = fs::write("dev/bench/icon_lookup_bench.json", json_content);
    println!("JSON results saved to dev/bench/icon_lookup_bench.json");
}
