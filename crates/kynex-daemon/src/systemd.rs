//! Safe implementation of systemd service readiness notification.
//!
//! Complies with the systemd service notification protocol (`sd_notify`) using 100% safe Rust.
//! Reads `$NOTIFY_SOCKET` and sends `READY=1\n` over a Unix datagram socket.

use std::io;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixDatagram};
use tracing::{debug, info, warn};

/// Sends the `READY=1\n` state notification to systemd via `$NOTIFY_SOCKET`.
///
/// If `$NOTIFY_SOCKET` is not set or empty, this function is a harmless no-op returning `Ok(())`.
/// Handles both regular filesystem paths and abstract socket addresses (prefixed with `@`).
///
/// # Errors
///
/// Returns an [`io::Error`] if creating, connecting, or sending over the datagram socket fails.
pub fn notify_ready() -> io::Result<()> {
    let notify_socket = match std::env::var("NOTIFY_SOCKET") {
        Ok(val) if !val.is_empty() => val,
        _ => {
            debug!("NOTIFY_SOCKET not set, skipping readiness notification");
            return Ok(());
        }
    };

    let sock = UnixDatagram::unbound()?;

    if let Some(abstract_name) = notify_socket.strip_prefix('@') {
        let addr = SocketAddr::from_abstract_name(abstract_name.as_bytes())?;
        sock.connect_addr(&addr)?;
    } else {
        sock.connect(&notify_socket)?;
    }

    let bytes_sent = sock.send(b"READY=1\n")?;
    if bytes_sent > 0 {
        info!("Notified systemd: READY=1 via socket '{notify_socket}'");
    } else {
        warn!("Failed to send bytes to systemd NOTIFY_SOCKET '{notify_socket}'");
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_notify_ready_noop_when_unset() {
        std::env::remove_var("NOTIFY_SOCKET");
        assert!(notify_ready().is_ok());
    }

    #[test]
    fn test_notify_ready_filesystem_socket() {
        let dir = tempdir().unwrap();
        let sock_path = dir.path().join("notify.sock");
        let listener = UnixDatagram::bind(&sock_path).unwrap();

        std::env::set_var("NOTIFY_SOCKET", &sock_path);
        let res = notify_ready();
        std::env::remove_var("NOTIFY_SOCKET");
        assert!(res.is_ok());

        let mut buf = [0u8; 64];
        let (len, _) = listener.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"READY=1\n");
    }
}
