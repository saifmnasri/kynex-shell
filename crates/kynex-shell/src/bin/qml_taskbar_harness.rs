//! QML Taskbar Replay Test Harness (Stage R4.1).
//!
//! Instantiates the REAL `styles/default/Shell.qml` from an installed-layout temp DESTDIR
//! with REAL `cxx-qt` taskbar/window models under `QT_QPA_PLATFORM=offscreen` and `QT_QUICK_BACKEND=software`.
//! Replays real captured window sequence and verifies scene correctness across 3 geometries.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::expect_used,
    clippy::unwrap_used
)]

use std::env;
use std::process;

fn print_usage() {
    eprintln!(
        "Usage: qml_taskbar_harness --destdir <PATH> [--geometry <WxH@DPR>] [--all-geometries]"
    );
    eprintln!("Geometries:");
    eprintln!("  1920x1080@1");
    eprintln!("  2520x1400@2");
    eprintln!("  3840x2160@2");
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    let _guard = rt.enter();

    let args: Vec<String> = env::args().collect();
    let mut destdir: Option<String> = None;
    let mut custom_geom: Option<(i32, i32, f64)> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--destdir" => {
                i += 1;
                if i < args.len() {
                    destdir = Some(args[i].clone());
                }
            }
            "--geometry" => {
                i += 1;
                if i < args.len() {
                    let g = &args[i];
                    if let Some((dims, dpr_str)) = g.split_once('@') {
                        if let Some((w_str, h_str)) = dims.split_once('x') {
                            if let (Ok(w), Ok(h), Ok(dpr)) = (
                                w_str.parse::<i32>(),
                                h_str.parse::<i32>(),
                                dpr_str.parse::<f64>(),
                            ) {
                                custom_geom = Some((w, h, dpr));
                            }
                        }
                    }
                }
            }
            "--all-geometries" => {
                custom_geom = None;
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                print_usage();
                process::exit(1);
            }
        }
        i += 1;
    }

    let target_destdir = destdir.unwrap_or_else(|| {
        if let Ok(d) = env::var("DESTDIR") {
            d
        } else {
            eprintln!("ERROR: --destdir <PATH> or DESTDIR env var is required");
            print_usage();
            process::exit(1);
        }
    });

    let geometries = if let Some(g) = custom_geom {
        vec![g]
    } else {
        vec![(1920, 1080, 1.0), (2520, 1400, 2.0), (3840, 2160, 2.0)]
    };

    println!(
        "[QML HARNESS RUNNER] Running {} geometry configurations against DESTDIR={}",
        geometries.len(),
        target_destdir
    );

    for (w, h, dpr) in geometries {
        println!("\n=======================================================");
        println!("[QML HARNESS RUNNER] Testing Geometry {w}x{h} DPR {dpr}");
        println!("=======================================================");

        let code = kynex_shell::shim::run_taskbar_qml_harness(&target_destdir, w, h, dpr);
        if code != 0 {
            eprintln!(
                "[QML HARNESS RUNNER] FAILED: Geometry {w}x{h} DPR {dpr} exited with code {code}"
            );
            process::exit(code);
        }
        println!("[QML HARNESS RUNNER] PASSED: Geometry {w}x{h} DPR {dpr}");
    }

    println!("\n[QML HARNESS RUNNER] ALL GEOMETRIES PASSED");
}
