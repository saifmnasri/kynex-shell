//! QML Session Replay & Verification Harness.
//!
//! Instantiates the REAL `styles/default/Shell.qml` from an installed-layout temp DESTDIR
//! with REAL `cxx-qt` models connected to the real daemon and fake logind on a private bus.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::expect_used,
    clippy::unwrap_used
)]

use std::env;
use std::process;

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    let _guard = rt.enter();

    let args: Vec<String> = env::args().collect();
    let mut destdir: Option<String> = None;
    let mut mode = "reproduce".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--destdir" => {
                i += 1;
                if i < args.len() {
                    destdir = Some(args[i].clone());
                }
            }
            "--mode" => {
                i += 1;
                if i < args.len() {
                    mode.clone_from(&args[i]);
                }
            }
            other => {
                eprintln!("Unknown argument: {other}");
                process::exit(1);
            }
        }
        i += 1;
    }

    let target_destdir = destdir.unwrap_or_else(|| {
        env::var("DESTDIR").unwrap_or_else(|_| {
            eprintln!("ERROR: --destdir <PATH> or DESTDIR env var is required");
            process::exit(1);
        })
    });

    let code = kynex_shell::shim::run_session_qml_harness(&target_destdir, &mode);
    process::exit(code);
}
