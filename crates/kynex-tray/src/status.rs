//! Status enum mapping for StatusNotifierItem status property.

/// Operational status of a StatusNotifierItem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ItemStatus {
    /// Item is passive (hidden or minimized in UI when quiescent).
    Passive,
    /// Item is active (normally visible in the system tray).
    #[default]
    Active,
    /// Item requests user attention (flashing/pulsing visual indicator).
    NeedsAttention,
}

impl ItemStatus {
    /// Returns the canonical specification string for this status.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Passive => "Passive",
            Self::Active => "Active",
            Self::NeedsAttention => "NeedsAttention",
        }
    }

    /// Parses a status string, defaulting to `Active` on unknown or malformed input.
    #[must_use]
    pub fn parse_or_default(input: &str) -> Self {
        match input.trim() {
            "Passive" => Self::Passive,
            "NeedsAttention" => Self::NeedsAttention,
            _ => Self::Active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_mapping() {
        assert_eq!(ItemStatus::parse_or_default("Passive"), ItemStatus::Passive);
        assert_eq!(ItemStatus::parse_or_default("Active"), ItemStatus::Active);
        assert_eq!(
            ItemStatus::parse_or_default("NeedsAttention"),
            ItemStatus::NeedsAttention
        );
        assert_eq!(ItemStatus::parse_or_default("unknown"), ItemStatus::Active);
        assert_eq!(ItemStatus::parse_or_default(""), ItemStatus::Active);

        assert_eq!(ItemStatus::Passive.as_str(), "Passive");
        assert_eq!(ItemStatus::Active.as_str(), "Active");
        assert_eq!(ItemStatus::NeedsAttention.as_str(), "NeedsAttention");
        assert_eq!(ItemStatus::default(), ItemStatus::Active);
    }
}
