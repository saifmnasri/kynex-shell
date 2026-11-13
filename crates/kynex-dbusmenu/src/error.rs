//! Error types for dbusmenu layout parsing, validation, and event handling.

use thiserror::Error;

/// Errors returned during dbusmenu layout validation and event dispatch.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum MenuError {
    /// Menu nesting depth exceeds maximum allowed bound.
    #[error("menu nesting depth {0} exceeds maximum allowed limit")]
    DepthExceeded(usize),

    /// Total menu item count exceeds maximum allowed bound.
    #[error("total menu item count {0} exceeds maximum allowed limit")]
    ItemCountExceeded(usize),

    /// Child count under a single node exceeds maximum allowed bound.
    #[error("child count {0} under node exceeds maximum allowed limit")]
    ChildCountExceeded(usize),

    /// A negative menu item ID was encountered (only root 0 is permitted).
    #[error("negative menu item ID {0} is prohibited")]
    NegativeId(i32),

    /// A duplicate menu item ID was encountered in the layout tree.
    #[error("duplicate menu item ID {0} detected in layout")]
    DuplicateId(i32),

    /// A cycle was detected in the menu hierarchy.
    #[error("cycle detected at menu item ID {0}")]
    CycleDetected(i32),

    /// A node is its own ancestor in the hierarchy.
    #[error("node ID {0} is its own ancestor")]
    SelfAncestor(i32),

    /// The root menu node is invalid or absent.
    #[error("root menu node is invalid")]
    RootInvalid,

    /// Target menu item ID does not exist in the current layout.
    #[error("menu item ID {0} not found in current layout")]
    ItemNotFound(i32),

    /// Event dispatch was attempted on a disabled menu item.
    #[error("menu item ID {0} is disabled")]
    ItemDisabled(i32),

    /// Event dispatch was attempted on a separator item.
    #[error("menu item ID {0} is a separator")]
    ItemIsSeparator(i32),

    /// An unsupported event kind was requested (only 'clicked' and 'hovered' are valid).
    #[error("unsupported menu event kind '{0}' (expected 'clicked' or 'hovered')")]
    InvalidEventKind(String),

    /// Event dispatch attempted against a stale revision.
    #[error("event on stale revision {requested_revision} (current is {layout_revision})")]
    StaleRevision {
        /// Revision of current active layout.
        layout_revision: u32,
        /// Revision supplied with event request.
        requested_revision: u32,
    },
}
