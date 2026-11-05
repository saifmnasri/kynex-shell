//! The Kynex background daemon responsible for hardware and session integration.

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use kynex_daemon::DaemonServer;
use kynex_ipc::{DAEMON_DESTINATION, DAEMON_PATH};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting kynex-daemon v{}", env!("CARGO_PKG_VERSION"));

    let running = Arc::new(AtomicBool::new(true));
    let server = DaemonServer::new(Arc::clone(&running));

    info!("Connecting to session D-Bus and claiming '{DAEMON_DESTINATION}'");
    let _conn = zbus::connection::Builder::session()
        .name(DAEMON_DESTINATION)
        .serve_at(DAEMON_PATH, server)
        .build()
        .await?;

    info!("kynex-daemon active and serving '{DAEMON_PATH}'");

    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => Some(s),
        Err(err) => {
            warn!("Failed to register SIGTERM handler: {err}");
            None
        }
    };

    tokio::select! {
        res = tokio::signal::ctrl_c() => {
            if let Err(err) = res {
                warn!("Error listening for SIGINT: {err}");
            } else {
                info!("Received SIGINT (Ctrl+C), shutting down cleanly");
            }
        }
        () = async {
            if let Some(ref mut s) = sigterm {
                s.recv().await;
            } else {
                std::future::pending::<()>().await;
            }
        } => {
            info!("Received SIGTERM, shutting down cleanly");
        }
    }

    running.store(false, Ordering::SeqCst);
    info!("kynex-daemon stopped");
    Ok(())
}
