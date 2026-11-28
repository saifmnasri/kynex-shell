//! Kynex Shell library exposing core models and C++ shim bridge.

pub mod audio;
pub mod clock;
pub mod debug;
pub mod launcher;
pub mod launcher_logic;
pub mod menu;
pub mod network;
pub mod notifications;
pub mod power;
pub mod session;
pub mod shim;
pub mod tray;
pub mod windows;

use tokio::sync::OnceCell;
use zbus::Connection;

static SHARED_SESSION_CONN: OnceCell<Connection> = OnceCell::const_new();

/// Returns a shared session bus connection for the shell process.
///
/// Ensures all exported shell objects (e.g. `QuickSettings`, `Debug1`) reside on the
/// same D-Bus connection owning the well-known name `org.kynexos.Shell`.
///
/// # Errors
/// Returns error if D-Bus session bus connection fails.
pub async fn shared_session_conn() -> zbus::Result<Connection> {
    SHARED_SESSION_CONN
        .get_or_try_init(|| async { zbus::connection::Builder::session().build().await })
        .await
        .cloned()
}
