//! Pure-Rust core library for dbusmenu layout and properties parsing,
//! validation, resource bounding, and event sanitization.
//!
//! Provides defense-in-depth against untrusted D-Bus menu trees:
//! - Hard resource limits (depth <= 6, total items <= 500, children per node <= 100).
//! - Cycle detection, duplicate rejection, and non-negative ID enforcement.
//! - String sanitization: drops ASCII control characters and Unicode bidi overrides/isolates,
//!   strips XML/HTML markup tags, and parses mnemonic accelerator underscores.
//! - Strict mapping of toggle kinds, toggle states, and dispositions.
//! - Event validation: dispatches only for valid, enabled, non-separator items.
//! - Pure Rust without Qt or D-Bus dependencies.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod event;
pub mod limits;
pub mod parser;
pub mod raw;
pub mod sanitize;
pub mod types;

pub use error::MenuError;
pub use event::validate_menu_event;
pub use limits::*;
pub use parser::parse_menu_layout;
pub use raw::{RawMenuNode, RawValue};
pub use sanitize::{
    format_and_sanitize_shortcut, parse_mnemonic_underscores, sanitize_icon_name, sanitize_label,
    strip_markup,
};
pub use types::{
    MenuDisposition, MenuItemKind, ToggleKind, ToggleState, ValidatedMenuItem, ValidatedMenuLayout,
};
