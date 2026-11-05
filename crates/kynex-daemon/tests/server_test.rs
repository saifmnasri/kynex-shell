//! Tests for kynex-daemon server implementation.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use kynex_daemon::DaemonServer;

#[tokio::test]
async fn test_daemon_server_direct() {
    let running = Arc::new(AtomicBool::new(true));
    let server = DaemonServer::new(running);

    // Verify status returns valid version and zero active services initially
    let status = server.status().await;
    assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(status.active_services, Vec::<String>::new());

    // Verify ping
    assert_eq!(server.ping().await, "pong");

    // Verify version
    assert_eq!(server.version().await, env!("CARGO_PKG_VERSION"));
}
