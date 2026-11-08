//! `kynex-apps-index`: UI-free XDG desktop entry indexer, parser, ranked search, and watcher.
//!
//! Complies with the Freedesktop.org Desktop Entry Specification (Version 1.5).
//! Features zero shell invocation, robust bounds checking, and pure event-driven directory watching.

pub mod entry;
pub mod exec;
pub mod index;
pub mod search;
pub mod watcher;

pub use entry::{
    decode_escapes, generate_locale_candidates, is_executable_in_path, parse_string_list,
    DesktopEntry, DesktopEntryError, LocalizedString, MAX_FILE_SIZE, MAX_GROUPS,
    MAX_KEYS_PER_GROUP, MAX_LINE_LENGTH,
};
pub use exec::{parse_exec, ExecContext, ExecError};
pub use index::AppsIndex;
pub use search::search_entries;
pub use watcher::AppsWatcher;
