//! Kynex style pack manifest, tokens parser, and validator.
//!
//! Style packs provide the presentation layer (QML layouts and design tokens)
//! for Kynex Shell while core provides logic, data models, and surface toolkits.

pub mod error;
pub mod loader;
pub mod manifest;
pub mod tokens;

pub use error::StyleError;
pub use loader::{load_style_pack, read_file_with_limit};
pub use manifest::{
    Archetype, AssetsConfig, EntryConfig, KWinConfig, StyleKind, StyleManifest, TokensConfig,
    MAX_STYLE_FILE_SIZE, STYLE_SCHEMA_VERSION,
};
pub use tokens::{
    ColorPalettes, MotionConfig, Palette, RadiusConfig, SpacingConfig, TokensManifest,
    TypographyConfig, MAX_TOKENS_FILE_SIZE, TOKENS_SCHEMA_VERSION,
};
