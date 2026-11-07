//! Internal library components for `kynex-daemon`.

pub mod server;
pub mod services;
pub mod window_state;

pub use server::DaemonServer;
pub use services::power::{PowerServer, PowerSnapshot};
