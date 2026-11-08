#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::cloned_ref_to_slice_refs,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]
//! Benchmark for 5,000 synthetic desktop entries (Stage K1.8).
//!
//! Measures:
//! - Cold index time (scan + parse 5,000 files)
//! - Incremental update time (single file update/modify)
//! - Search latency (p50 and p99 over diverse queries)

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use kynex_apps_index::index::AppsIndex;

fn generate_synthetic_entries(dir: &Path, count: usize) {
    fs::create_dir_all(dir).expect("failed to create bench dir");

    let categories = [
        "AudioVideo;Audio;",
        "Development;IDE;",
        "Education;Science;",
        "Game;ArcadeGame;",
        "Graphics;2DGraphics;RasterGraphics;",
        "Network;WebBrowser;",
        "Office;WordProcessor;",
        "Settings;DesktopSettings;",
        "System;TerminalEmulator;",
        "Utility;TextEditor;Calculator;",
    ];

    let generic_names = [
        "Audio Player",
        "Integrated Development Environment",
        "Scientific Calculator",
        "Arcade Classic Game",
        "Image Editor",
        "Web Browser",
        "Document Editor",
        "System Configuration",
        "Terminal Emulator",
        "Text Editor",
    ];

    for i in 1..=count {
        let cat_idx = (i - 1) % categories.len();
        let filename = format!("app_{i:05}.desktop");
        let path = dir.join(&filename);

        let content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Kynex Synthetic Application {i}\n\
             GenericName={generic}\n\
             Comment=Enterprise high-speed application number {i} for KynexOS\n\
             Icon=application-default-icon\n\
             Exec=kynex-app-{i} --flag %f %u\n\
             Categories={cat}\n\
             Keywords=kynex;enterprise;suite;app{i};fast;\n\
             StartupNotify=true\n",
            generic = generic_names[cat_idx],
            cat = categories[cat_idx],
        );

        let mut file = File::create(&path).expect("failed to create file");
        file.write_all(content.as_bytes())
            .expect("failed to write file");
    }
}

fn main() {
    let count = 5_000;
    let temp_base = std::env::temp_dir().join(format!("kynex_bench_5000_{}", std::process::id()));
    let apps_dir = temp_base.join("applications");

    println!("=== K1.8 Benchmark: 5,000 Synthetic Desktop Entries ===");
    println!(
        "Generating {count} synthetic .desktop entries in {}...",
        apps_dir.display()
    );
    let gen_start = Instant::now();
    generate_synthetic_entries(&apps_dir, count);
    println!("Generated {count} files in {:.2?}", gen_start.elapsed());

    let dirs = [apps_dir.clone()];

    // 1. Cold Index Time
    println!("\n--- 1. Cold Index Time (5 runs) ---");
    let mut cold_times = Vec::new();
    let mut loaded_index = AppsIndex::new("Kynex");
    for run in 1..=5 {
        let start = Instant::now();
        let idx = AppsIndex::load_from_dirs(&dirs, "Kynex").expect("load_from_dirs failed");
        let elapsed = start.elapsed();
        cold_times.push(elapsed.as_secs_f64() * 1000.0);
        println!(
            "  Run {run}: {:.2} ms (entries: {})",
            elapsed.as_secs_f64() * 1000.0,
            idx.len()
        );
        if run == 5 {
            loaded_index = idx;
        }
    }
    cold_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let cold_median = cold_times[cold_times.len() / 2];
    let cold_mean: f64 = cold_times.iter().sum::<f64>() / cold_times.len() as f64;
    println!(
        "Cold Index: Median = {cold_median:.2} ms, Mean = {cold_mean:.2} ms, Min = {:.2} ms",
        cold_times[0]
    );

    // 2. Incremental Update Time (100 iterations)
    println!("\n--- 2. Incremental Update Time (100 iterations) ---");
    let target_file = apps_dir.join("app_02500.desktop");
    let mut inc_times_us = Vec::new();
    for iter in 1..=100 {
        // Touch/modify file
        let content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Updated Synthetic Application 2500 (iteration {iter})\n\
             GenericName=Updated Generic\n\
             Icon=icon-updated\n\
             Exec=updated-cmd %u\n\
             Categories=Utility;\n"
        );
        fs::write(&target_file, content.as_bytes()).expect("failed to update file");

        let start = Instant::now();
        loaded_index.update_entry_file("app_02500.desktop", &target_file);
        let elapsed_us = start.elapsed().as_micros() as f64;
        inc_times_us.push(elapsed_us);
    }
    inc_times_us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let inc_p50 = inc_times_us[(inc_times_us.len() as f64 * 0.50) as usize] / 1000.0;
    let inc_p99 = inc_times_us[(inc_times_us.len() as f64 * 0.99) as usize] / 1000.0;
    println!(
        "Incremental Update: p50 = {inc_p50:.3} ms, p99 = {inc_p99:.3} ms, max = {:.3} ms",
        inc_times_us.last().unwrap() / 1000.0
    );

    // 3. Search Latency (2,000 queries)
    println!("\n--- 3. Search Latency (2,000 diverse queries) ---");
    let test_queries = [
        "web",
        "doc",
        "calc",
        "term",
        "edit",
        "graph",
        "sys",
        "app",
        "app_0499",
        "kynex",
        "fast",
        "science",
        "word",
        "video",
        "arcade",
        "settings",
        "browser",
        "terminal",
        "audio",
        "player",
        "image",
        "suite",
        "enterprise",
        "classic",
        "1",
    ];

    let mut search_latencies_us = Vec::with_capacity(2000);
    for i in 0..2000 {
        let q = test_queries[i % test_queries.len()];
        let limit = 50;
        let start = Instant::now();
        let _results = loaded_index.search(q, limit, None);
        let elapsed_us = start.elapsed().as_micros() as f64;
        search_latencies_us.push(elapsed_us);
    }

    search_latencies_us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = search_latencies_us[(search_latencies_us.len() as f64 * 0.50) as usize] / 1000.0;
    let p90 = search_latencies_us[(search_latencies_us.len() as f64 * 0.90) as usize] / 1000.0;
    let p95 = search_latencies_us[(search_latencies_us.len() as f64 * 0.95) as usize] / 1000.0;
    let p99 = search_latencies_us[(search_latencies_us.len() as f64 * 0.99) as usize] / 1000.0;
    let max = search_latencies_us.last().unwrap() / 1000.0;

    println!("Search Latency (5,000 entries, limit 50):");
    println!("  p50: {p50:.3} ms");
    println!("  p90: {p90:.3} ms");
    println!("  p95: {p95:.3} ms");
    println!(
        "  p99: {p99:.3} ms (budget target < 16.0 ms: {})",
        if p99 < 16.0 { "PASS" } else { "FAIL" }
    );
    println!("  max: {max:.3} ms");

    // Clean up tempdir
    let _ = fs::remove_dir_all(&temp_base);

    println!("\n=== Summary Results ===");
    println!("COLD_INDEX_MS={cold_median:.2}");
    println!("INCREMENTAL_UPDATE_P50_MS={inc_p50:.3}");
    println!("INCREMENTAL_UPDATE_P99_MS={inc_p99:.3}");
    println!("SEARCH_LATENCY_P50_MS={p50:.3}");
    println!("SEARCH_LATENCY_P99_MS={p99:.3}");
}
