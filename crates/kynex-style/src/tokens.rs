//! Design tokens representation and validation.

use crate::error::StyleError;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current supported tokens schema version.
pub const TOKENS_SCHEMA_VERSION: u32 = 1;

/// Maximum allowed file size for tokens.toml (1 MiB).
pub const MAX_TOKENS_FILE_SIZE: u64 = 1024 * 1024;

/// Complete design tokens definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokensManifest {
    /// Schema version of the tokens file.
    pub schema_version: u32,
    /// Color palettes for dark and light themes.
    pub color: ColorPalettes,
    /// Spacing constants.
    pub spacing: SpacingConfig,
    /// Corner radii.
    pub radius: RadiusConfig,
    /// Motion durations in milliseconds.
    pub motion: MotionConfig,
    /// Typography configuration.
    pub typography: TypographyConfig,
}

/// Color palettes for dark and light modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorPalettes {
    /// Dark theme colors.
    pub dark: Palette,
    /// Light theme colors.
    pub light: Palette,
}

/// Color tokens for a single theme mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    /// Base background surface color.
    pub surface: String,
    /// Elevated/card surface color.
    pub surface_raised: String,
    /// Primary text color.
    pub text: String,
    /// Muted/secondary text color.
    pub text_muted: String,
    /// Accent/highlight color.
    pub accent: String,
    /// Error/danger state color.
    pub danger: String,
    /// Warning state color.
    #[serde(default = "default_warning_color")]
    pub warning: String,
    /// Success state color.
    pub success: String,
}

fn default_warning_color() -> String {
    "#e5a50a".to_string()
}

/// Spacing system tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpacingConfig {
    /// Base grid unit in pixels.
    pub unit: u32,
    /// Small spacing in pixels.
    #[serde(default = "default_small_spacing")]
    pub small: u32,
    /// Medium spacing in pixels.
    #[serde(default = "default_medium_spacing")]
    pub medium: u32,
    /// Large spacing in pixels.
    #[serde(default = "default_large_spacing")]
    pub large: u32,
    /// Panel height/thickness in pixels.
    #[serde(default = "default_panel_spacing")]
    pub panel: u32,
}

fn default_small_spacing() -> u32 {
    8
}

fn default_medium_spacing() -> u32 {
    12
}

fn default_large_spacing() -> u32 {
    16
}

fn default_panel_spacing() -> u32 {
    48
}

/// Corner radius tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadiusConfig {
    /// Small radius in pixels.
    pub small: u32,
    /// Medium radius in pixels.
    pub medium: u32,
    /// Large radius in pixels.
    pub large: u32,
}

/// Motion and animation duration tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionConfig {
    /// Fast transition duration in ms.
    pub fast_ms: u32,
    /// Standard transition duration in ms.
    pub normal_ms: u32,
    /// Slow transition duration in ms.
    pub slow_ms: u32,
}

/// Typography tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypographyConfig {
    /// Primary font family name.
    pub family: String,
    /// Fallback font family generic.
    pub fallback: String,
    /// Base font size in pixels.
    pub base_px: u32,
}

impl TokensManifest {
    /// Validate all values in the tokens manifest.
    ///
    /// # Errors
    ///
    /// Returns a [`StyleError`] if schema version is unsupported or color tokens are invalid hex strings.
    pub fn validate(&self, path: &Path) -> Result<(), StyleError> {
        if self.schema_version != TOKENS_SCHEMA_VERSION {
            return Err(StyleError::UnsupportedSchemaVersion {
                path: path.to_path_buf(),
                version: self.schema_version,
                expected: TOKENS_SCHEMA_VERSION,
            });
        }

        Self::validate_palette(&self.color.dark)?;
        Self::validate_palette(&self.color.light)?;

        Ok(())
    }

    fn validate_palette(palette: &Palette) -> Result<(), StyleError> {
        let colors = [
            &palette.surface,
            &palette.surface_raised,
            &palette.text,
            &palette.text_muted,
            &palette.accent,
            &palette.danger,
            &palette.warning,
            &palette.success,
        ];

        for &c in &colors {
            if !is_valid_hex_color(c) {
                return Err(StyleError::InvalidColor { color: c.clone() });
            }
        }
        Ok(())
    }
}

/// Validates whether a string is a valid CSS-style hex color (#RGB, #RRGGBB, #RRGGBBAA).
#[must_use]
pub fn is_valid_hex_color(s: &str) -> bool {
    if !s.starts_with('#') {
        return false;
    }
    let hex = &s[1..];
    (hex.len() == 3 || hex.len() == 6 || hex.len() == 8)
        && hex.chars().all(|c| c.is_ascii_hexdigit())
}
