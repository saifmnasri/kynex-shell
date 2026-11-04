//! D-Bus interface definitions and IPC protocols shared by the Kynex daemon and clients.

use kynex_common::NAMESPACE;

/// The base object path for `KynexOS` D-Bus objects.
pub const OBJECT_PATH_PREFIX: &str = "/org/kynexos";

/// Returns the formatted D-Bus interface name for a given interface suffix.
#[must_use]
pub fn interface_name(suffix: &str) -> String {
    format!("{NAMESPACE}.{suffix}")
}
