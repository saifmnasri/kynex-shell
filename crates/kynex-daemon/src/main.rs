//! The Kynex background daemon responsible for hardware and session integration.

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use kynex_daemon::services::power::{run_power_service, PowerServer};
use kynex_daemon::DaemonServer;
use kynex_ipc::{DAEMON_DESTINATION, DAEMON_PATH, POWER_PATH};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{info, warn};
use zbus::object_server::SignalEmitter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting kynex-daemon v{}", env!("CARGO_PKG_VERSION"));

    let running = Arc::new(AtomicBool::new(true));
    let server = DaemonServer::new(Arc::clone(&running));
    let power_server = PowerServer::new();

    info!("Connecting to session D-Bus and claiming '{DAEMON_DESTINATION}'");
    let session_conn = zbus::connection::Builder::session()
        .name(DAEMON_DESTINATION)
        .serve_at(DAEMON_PATH, server)
        .serve_at(POWER_PATH, power_server.clone())
        .build()
        .await?;

    info!("kynex-daemon active and serving '{DAEMON_PATH}'");

    // Create emitter for power property change signals on session bus
    let emitter = match SignalEmitter::new(&session_conn, POWER_PATH) {
        Ok(em) => Some(em),
        Err(err) => {
            warn!("Failed to create SignalEmitter for Power1: {err}");
            None
        }
    };

    // Determine UPower connection:
    // If KYNEX_UPOWER_BUS == "session", use session bus (for dev/nested demo/tests).
    // Otherwise, connect to system bus.
    let upower_conn = if std::env::var("KYNEX_UPOWER_BUS").as_deref() == Ok("session") {
        info!("KYNEX_UPOWER_BUS=session: using session bus for UPower");
        Some(session_conn.clone())
    } else {
        match zbus::connection::Builder::system().build().await {
            Ok(conn) => Some(conn),
            Err(err) => {
                warn!("Failed to connect to system bus for UPower: {err}. Battery status will be unavailable (present=false).");
                None
            }
        }
    };

    // Spawn power monitoring background task if UPower bus connection is available
    if let Some(uconn) = upower_conn {
        let p_server = power_server.clone();
        tokio::spawn(async move {
            run_power_service(uconn, p_server, emitter, None).await;
        });
    }

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
