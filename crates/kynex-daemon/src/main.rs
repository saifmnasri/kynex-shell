//! The Kynex background daemon responsible for hardware and session integration.

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use kynex_daemon::services::apps::{setup_apps_watcher, AppsServer, AppsSharedState};
use kynex_daemon::services::network::{run_network_service, NetworkServer};
use kynex_daemon::services::power::{run_power_service, PowerServer};
use kynex_daemon::services::windows::{
    run_kwin_watcher, AllowAllSenderPolicy, KWinSenderPolicy, SenderPolicy, WindowsFeedServer,
    WindowsServer, WindowsSharedState,
};
use kynex_daemon::DaemonServer;
use kynex_ipc::{
    apps::APPS_PATH, DAEMON_DESTINATION, DAEMON_PATH, NETWORK_PATH, POWER_PATH, WINDOWS_PATH,
};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{info, warn};
use zbus::object_server::SignalEmitter;

#[allow(clippy::too_many_lines)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting kynex-daemon v{}", env!("CARGO_PKG_VERSION"));

    let running = Arc::new(AtomicBool::new(true));
    let server = DaemonServer::new(Arc::clone(&running));
    let power_server = PowerServer::new();
    let network_server = NetworkServer::new();

    let sender_policy: Arc<dyn SenderPolicy> =
        if std::env::var("KYNEX_ALLOW_TEST_FEEDER").unwrap_or_default() == "1" {
            warn!("KYNEX_ALLOW_TEST_FEEDER=1: using permissive AllowAllSenderPolicy");
            Arc::new(AllowAllSenderPolicy)
        } else {
            Arc::new(KWinSenderPolicy::new())
        };
    let windows_shared = Arc::new(WindowsSharedState::new(sender_policy));
    let windows_server = WindowsServer::new(Arc::clone(&windows_shared));
    let windows_feed_server = WindowsFeedServer::new(Arc::clone(&windows_shared));

    let app_dirs = kynex_apps_index::AppsIndex::standard_app_dirs();
    let current_desktop =
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "Kynex".to_string());
    let apps_shared = Arc::new(AppsSharedState::new(app_dirs, current_desktop));
    let apps_server = AppsServer::new(Arc::clone(&apps_shared));

    info!("Connecting to session D-Bus and claiming '{DAEMON_DESTINATION}'");
    let session_conn = zbus::connection::Builder::session()
        .name(DAEMON_DESTINATION)
        .serve_at(DAEMON_PATH, server)
        .serve_at(POWER_PATH, power_server.clone())
        .serve_at(NETWORK_PATH, network_server.clone())
        .serve_at(WINDOWS_PATH, windows_server.clone())
        .serve_at(WINDOWS_PATH, windows_feed_server.clone())
        .serve_at(APPS_PATH, apps_server)
        .build()
        .await?;

    info!("kynex-daemon active and serving '{DAEMON_PATH}'");

    // Initialize KWin script action executor on session bus
    windows_shared
        .set_action_executor(Arc::new(
            kynex_daemon::services::windows::KWinScriptActionExecutor::new(session_conn.clone()),
        ))
        .await;

    // Create emitter for window change signals on session bus
    if let Ok(windows_emitter) = SignalEmitter::new(&session_conn, WINDOWS_PATH) {
        windows_feed_server
            .set_emitter(windows_emitter.clone())
            .await;
        tokio::spawn(run_kwin_watcher(
            session_conn.clone(),
            windows_server.clone(),
            windows_emitter,
        ));
    } else {
        warn!("Failed to create SignalEmitter for Windows1");
    }

    // Set up application index directory watcher and signal emitter
    let _apps_watcher = if let Ok(apps_emitter) = SignalEmitter::new(&session_conn, APPS_PATH) {
        setup_apps_watcher(Arc::clone(&apps_shared), apps_emitter)
    } else {
        warn!("Failed to create SignalEmitter for Apps1");
        None
    };

    // Create emitter for power property change signals on session bus
    let emitter = match SignalEmitter::new(&session_conn, POWER_PATH) {
        Ok(em) => Some(em),
        Err(err) => {
            warn!("Failed to create SignalEmitter for Power1: {err}");
            None
        }
    };

    // Create emitter for network property change signals on session bus
    let network_emitter = match SignalEmitter::new(&session_conn, NETWORK_PATH) {
        Ok(em) => Some(em),
        Err(err) => {
            warn!("Failed to create SignalEmitter for Network1: {err}");
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

    // Determine NetworkManager connection:
    // If KYNEX_NM_BUS == "session", use session bus (for dev/nested demo/tests).
    // Otherwise, connect to system bus.
    let nm_conn = if std::env::var("KYNEX_NM_BUS").as_deref() == Ok("session") {
        info!("KYNEX_NM_BUS=session: using session bus for NetworkManager");
        Some(session_conn.clone())
    } else {
        match zbus::connection::Builder::system().build().await {
            Ok(conn) => Some(conn),
            Err(err) => {
                warn!("Failed to connect to system bus for NetworkManager: {err}. Network status will be unavailable (present=false).");
                None
            }
        }
    };

    // Spawn network monitoring background task if NetworkManager bus connection is available
    if let Some(nconn) = nm_conn {
        let n_server = network_server.clone();
        tokio::spawn(async move {
            run_network_service(nconn, n_server, network_emitter, None, None).await;
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
