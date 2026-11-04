//! The Kynex background daemon responsible for hardware and session integration.

use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting kynex-daemon (Phase 0 skeleton)");
    Ok(())
}
