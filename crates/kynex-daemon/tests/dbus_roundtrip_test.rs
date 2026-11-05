//! End-to-end integration test: connecting to `DaemonServer` via zbus on the session bus.

#![allow(clippy::print_stderr)]

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use kynex_daemon::DaemonServer;
use kynex_ipc::Daemon1Proxy;

const TEST_DESTINATION: &str = "org.kynexos.DaemonTest";
const TEST_PATH: &str = "/org/kynexos/DaemonTest";

#[tokio::test]
async fn test_dbus_session_bus_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // Only run if DBUS_SESSION_BUS_ADDRESS is present
    if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
        eprintln!("Skipping test: DBUS_SESSION_BUS_ADDRESS not set");
        return Ok(());
    }

    let running = Arc::new(AtomicBool::new(true));
    let server = DaemonServer::new(running);

    // Build server connection and claim test service name
    let server_conn = zbus::connection::Builder::session()
        .name(TEST_DESTINATION)
        .serve_at(TEST_PATH, server)
        .build()
        .await?;

    // Build client connection to call the server
    let client_conn = zbus::connection::Builder::session().build().await?;

    let proxy = Daemon1Proxy::builder(&client_conn)
        .destination(TEST_DESTINATION)
        .path(TEST_PATH)
        .build()
        .await?;

    // Verify ping method
    let pong = proxy.ping().await?;
    assert_eq!(pong, "pong");

    // Verify version method
    let version = proxy.version().await?;
    assert_eq!(version, env!("CARGO_PKG_VERSION"));

    // Verify status method
    let status = proxy.status().await?;
    assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(status.active_services, Vec::<String>::new());

    drop(proxy);
    drop(client_conn);
    drop(server_conn);
    Ok(())
}
