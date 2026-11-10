//! Pure-Rust business logic for the shell taskbar: pinned app ordering,
//! window grouping, multi-window cycling, and interaction dispatch.

use std::collections::{HashMap, HashSet};

use kynex_ipc::windows::WindowItem;

use crate::launcher::{compute_letter_badge, sanitize_display_text, MAX_NAME_LEN};
use crate::matching::{match_window_to_app, strip_desktop_suffix, AppCandidate};

/// Representation of an application item on the shell taskbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskbarEntry {
    /// Desktop entry ID (e.g. `org.gnome.Terminal.desktop`).
    pub app_id: String,
    /// Sanitized display name (for tooltips and accessibility).
    pub name: String,
    /// Icon name or path.
    pub icon_name: String,
    /// Single-character fallback letter badge.
    pub letter_badge: String,
    /// Whether this entry is in the user's pinned apps list.
    pub is_pinned: bool,
    /// Whether this application currently has one or more open windows.
    pub is_running: bool,
    /// Whether the active/focused window belongs to this application.
    pub is_active: bool,
    /// Number of open windows belonging to this application.
    pub window_count: usize,
    /// List of window IDs belonging to this application.
    pub window_ids: Vec<String>,
}

/// Action resulting from clicking a taskbar item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskbarAction {
    /// Launch the application via `Apps1.Launch`.
    Launch {
        /// Desktop application ID.
        app_id: String,
    },
    /// Activate/focus the window via `Windows1.Activate`.
    Activate {
        /// Compositor window ID.
        window_id: String,
    },
    /// Minimize the window via `Windows1.Minimize`.
    Minimize {
        /// Compositor window ID.
        window_id: String,
    },
    /// Close the window via `Windows1.Close`.
    Close {
        /// Compositor window ID.
        window_id: String,
    },
    /// No action required.
    None,
}

/// Pure-Rust state machine and aggregation engine for the shell taskbar.
#[derive(Debug, Default, Clone)]
pub struct TaskbarCore {
    pinned_ids: Vec<String>,
    candidates: Vec<AppCandidate>,
    candidate_names: HashMap<String, String>,
    windows: Vec<WindowItem>,
    entries: Vec<TaskbarEntry>,
}

impl TaskbarCore {
    /// Create a new empty taskbar core.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the pinned applications list.
    pub fn set_pinned_ids(&mut self, pinned_ids: Vec<String>) {
        self.pinned_ids = pinned_ids;
        self.recompute();
    }

    /// Update the known applications catalog for matching and metadata.
    pub fn set_candidates(
        &mut self,
        candidates: Vec<AppCandidate>,
        names: HashMap<String, String>,
    ) {
        self.candidates = candidates;
        self.candidate_names = names;
        self.recompute();
    }

    /// Update the list of running windows.
    pub fn set_windows(&mut self, windows: Vec<WindowItem>) {
        self.windows = windows;
        self.recompute();
    }

    /// Access the computed list of taskbar entries.
    #[must_use]
    pub fn entries(&self) -> &[TaskbarEntry] {
        &self.entries
    }

    /// Recompute taskbar entries: pinned apps first, then unpinned running apps.
    pub fn recompute(&mut self) {
        // Group windows by matched app_id
        let mut app_windows: HashMap<String, Vec<&WindowItem>> = HashMap::new();
        // Track the order in which unpinned running apps first appeared
        let mut unpinned_running_order: Vec<String> = Vec::new();
        let pinned_set: HashSet<&str> = self.pinned_ids.iter().map(String::as_str).collect();

        for win in &self.windows {
            let matched_id =
                if let Some(candidate) = match_window_to_app(&win.app_id, &self.candidates) {
                    candidate.id.clone()
                } else {
                    // If unindexed, normalize or fallback to raw app_id
                    win.app_id.clone()
                };

            let entry = app_windows.entry(matched_id.clone()).or_default();
            entry.push(win);

            if !pinned_set.contains(matched_id.as_str())
                && !unpinned_running_order.contains(&matched_id)
            {
                unpinned_running_order.push(matched_id);
            }
        }

        let mut computed = Vec::new();

        // 1. Pinned apps in their exact pin order
        for pin_id in &self.pinned_ids {
            let matching_windows = app_windows.remove(pin_id.as_str()).unwrap_or_default();
            let is_running = !matching_windows.is_empty();
            let is_active = matching_windows.iter().any(|w| w.active);
            let window_ids = matching_windows.iter().map(|w| w.id.clone()).collect();
            let window_count = matching_windows.len();

            let (name, icon_name) = self.resolve_metadata(pin_id);
            let letter_badge = compute_letter_badge(&name, pin_id);

            computed.push(TaskbarEntry {
                app_id: pin_id.clone(),
                name,
                icon_name,
                letter_badge,
                is_pinned: true,
                is_running,
                is_active,
                window_count,
                window_ids,
            });
        }

        // 2. Unpinned running apps in order of appearance
        for app_id in unpinned_running_order {
            if let Some(matching_windows) = app_windows.remove(app_id.as_str()) {
                let is_active = matching_windows.iter().any(|w| w.active);
                let window_ids = matching_windows.iter().map(|w| w.id.clone()).collect();
                let window_count = matching_windows.len();

                let (name, icon_name) = self.resolve_metadata(&app_id);
                let letter_badge = compute_letter_badge(&name, &app_id);

                computed.push(TaskbarEntry {
                    app_id,
                    name,
                    icon_name,
                    letter_badge,
                    is_pinned: false,
                    is_running: true,
                    is_active,
                    window_count,
                    window_ids,
                });
            }
        }

        self.entries = computed;
    }

    /// Dispatches a left-click on the taskbar item at `index`.
    #[must_use]
    pub fn handle_click(&self, index: usize) -> TaskbarAction {
        let Some(entry) = self.entries.get(index) else {
            return TaskbarAction::None;
        };

        if !entry.is_running || entry.window_ids.is_empty() {
            return TaskbarAction::Launch {
                app_id: entry.app_id.clone(),
            };
        }

        if entry.window_count == 1 {
            let win_id = entry.window_ids[0].clone();
            if entry.is_active {
                TaskbarAction::Minimize { window_id: win_id }
            } else {
                TaskbarAction::Activate { window_id: win_id }
            }
        } else {
            // Multiple windows: cycle through them.
            let active_idx = entry
                .window_ids
                .iter()
                .position(|id| self.windows.iter().any(|w| &w.id == id && w.active));

            match active_idx {
                Some(curr) => {
                    let next_idx = (curr + 1) % entry.window_ids.len();
                    TaskbarAction::Activate {
                        window_id: entry.window_ids[next_idx].clone(),
                    }
                }
                None => TaskbarAction::Activate {
                    window_id: entry.window_ids[0].clone(),
                },
            }
        }
    }

    /// Dispatches a middle-click on the taskbar item at `index`.
    #[must_use]
    pub fn handle_middle_click(&self, index: usize) -> TaskbarAction {
        let Some(entry) = self.entries.get(index) else {
            return TaskbarAction::None;
        };

        if !entry.is_running || entry.window_ids.is_empty() {
            return TaskbarAction::None;
        }

        // Close currently active window if active, otherwise close the most recent window
        let active_win = entry
            .window_ids
            .iter()
            .find(|id| self.windows.iter().any(|w| &w.id == *id && w.active));

        let target_id = active_win.unwrap_or(&entry.window_ids[entry.window_ids.len() - 1]);
        TaskbarAction::Close {
            window_id: target_id.clone(),
        }
    }

    fn resolve_metadata(&self, app_id: &str) -> (String, String) {
        let name = if let Some(n) = self.candidate_names.get(app_id) {
            sanitize_display_text(n, MAX_NAME_LEN)
        } else {
            let fallback = strip_desktop_suffix(app_id);
            sanitize_display_text(fallback, MAX_NAME_LEN)
        };

        let icon_name = self.candidates.iter().find(|c| c.id == app_id).map_or_else(
            || strip_desktop_suffix(app_id).to_string(),
            |c| c.icon_name.clone(),
        );

        (name, icon_name)
    }
}
