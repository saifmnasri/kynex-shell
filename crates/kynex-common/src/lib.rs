//! Shared constants, configuration schemas, and common types for `KynexOS`.
//!
//! This crate contains pure data types and schemas with no direct I/O dependencies.

/// The standard D-Bus and application namespace for `KynexOS`.
pub const NAMESPACE: &str = "org.kynexos";

/// The default daemon service name.
pub const DAEMON_SERVICE_NAME: &str = "org.kynexos.Daemon";

/// The default style pack identifier.
pub const DEFAULT_STYLE_ID: &str = "default";

/// The default percentage threshold below which battery level is considered low (warning).
pub const BATTERY_LOW_PERCENTAGE: f64 = 20.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_battery_low_threshold() {
        const {
            assert!(BATTERY_LOW_PERCENTAGE > 0.0 && BATTERY_LOW_PERCENTAGE < 100.0);
        }
        assert!((BATTERY_LOW_PERCENTAGE - 20.0).abs() < f64::EPSILON);
    }
}
