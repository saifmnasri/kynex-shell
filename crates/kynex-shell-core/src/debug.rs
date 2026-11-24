//! Pure-Rust sanitized state representation for `org.kynexos.Shell.Debug1`.
//!
//! # Security Invariant
//! Window titles, notification bodies/summaries, and any user-entered content
//! are strictly omitted from `ShellDebugState` and its serialized JSON representation.
//! Only counts, system IDs, application desktop IDs, state flags, and timestamps are exposed.

use serde::{Deserialize, Serialize};

/// Readiness state across shell sub-models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ModelReadinessState {
    /// Whether window tracking is synced.
    pub windows: bool,
    /// Whether application index is synced.
    pub apps: bool,
    /// Whether pinned applications are loaded.
    pub pins: bool,
    /// Whether audio model is connected.
    pub audio: bool,
    /// Whether network model is connected.
    pub network: bool,
    /// Whether power model is connected.
    pub power: bool,
    /// Whether notifications model is connected.
    pub notifications: bool,
    /// Whether system tray model is connected.
    pub tray: bool,
    /// Whether session and power model is connected.
    pub session: bool,
}

/// Sanitized representation of a single taskbar entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct TaskbarEntryDebugState {
    /// Desktop application ID.
    pub app_id: String,
    /// Whether the app is in the user's pinned apps list.
    pub is_pinned: bool,
    /// Whether the app has running windows.
    pub is_running: bool,
    /// Whether the app is currently focused/active.
    pub is_active: bool,
    /// Whether all open windows for this app are minimized.
    pub is_minimized: bool,
    /// Number of open windows belonging to this app.
    pub window_count: usize,
    /// Compositor internal IDs of windows belonging to this app.
    pub window_ids: Vec<String>,
    /// Fallback letter badge.
    pub letter_badge: String,
}

/// Sanitized aggregate taskbar state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaskbarDebugState {
    /// Total number of taskbar entries (pinned + running unpinned).
    pub entry_count: usize,
    /// Total number of tracked running windows.
    pub window_count: usize,
    /// Number of pinned entries.
    pub pinned_count: usize,
    /// Entries on the taskbar.
    pub entries: Vec<TaskbarEntryDebugState>,
}

/// Sanitized window tracking entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowDebugState {
    /// Compositor internal ID.
    pub id: String,
    /// Application ID.
    pub app_id: String,
    /// Whether the window is minimized.
    pub minimized: bool,
    /// Whether the window has active focus.
    pub active: bool,
}

/// Sanitized notifications state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NotificationDebugState {
    /// Number of active notifications.
    pub active_count: usize,
    /// Active notification IDs.
    pub notification_ids: Vec<u32>,
}

/// Sanitized session and power state.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionDebugState {
    /// Whether logout is available.
    pub can_logout: bool,
    /// Whether restart is available.
    pub can_restart: bool,
    /// Whether power off is available.
    pub can_power_off: bool,
    /// Whether suspend is available.
    pub can_suspend: bool,
    /// Whether overlay is open.
    pub overlay_open: bool,
    /// Confirmation state ("none", "restart", "`power_off`").
    pub confirmation: String,
}

/// Complete sanitized snapshot of shell state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellDebugState {
    /// Whether the shell has an active connection to `kynex-daemon`.
    pub daemon_connected: bool,
    /// Whether primary models have completed initial synchronization.
    pub models_ready: bool,
    /// ISO 8601 or monotonic timestamp of last resync.
    pub last_resync_time: Option<String>,
    /// Sub-model readiness flags.
    pub models: ModelReadinessState,
    /// Taskbar presentation state.
    pub taskbar: TaskbarDebugState,
    /// Raw tracked window entries (no titles).
    pub windows: Vec<WindowDebugState>,
    /// Notifications state (no bodies or summaries).
    pub notifications: NotificationDebugState,
    /// Session and power state.
    pub session: SessionDebugState,
}

impl ShellDebugState {
    /// Serializes this snapshot into a sanitized JSON string.
    ///
    /// # Errors
    /// Returns `Err` if JSON serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_dump_state_contains_no_titles_or_bodies() {
        let sensitive_window_title = "Secret Bank Statement - Firefox";
        let sensitive_notif_body = "Your verification code is 987654. Do not share.";

        let state = ShellDebugState {
            daemon_connected: true,
            models_ready: true,
            last_resync_time: Some("2026-11-19T12:00:00Z".to_string()),
            models: ModelReadinessState {
                windows: true,
                apps: true,
                pins: true,
                audio: true,
                network: true,
                power: true,
                notifications: true,
                tray: true,
                session: true,
            },
            taskbar: TaskbarDebugState {
                entry_count: 1,
                window_count: 1,
                pinned_count: 0,
                entries: vec![TaskbarEntryDebugState {
                    app_id: "org.mozilla.firefox".to_string(),
                    is_pinned: false,
                    is_running: true,
                    is_active: true,
                    is_minimized: false,
                    window_count: 1,
                    window_ids: vec!["{1111-2222-3333-4444}".to_string()],
                    letter_badge: "F".to_string(),
                }],
            },
            windows: vec![WindowDebugState {
                id: "{1111-2222-3333-4444}".to_string(),
                app_id: "org.mozilla.firefox".to_string(),
                minimized: false,
                active: true,
            }],
            notifications: NotificationDebugState {
                active_count: 1,
                notification_ids: vec![42],
            },
            session: SessionDebugState {
                can_logout: true,
                can_restart: true,
                can_power_off: true,
                can_suspend: true,
                overlay_open: false,
                confirmation: "none".to_string(),
            },
        };

        let json = state.to_json().expect("Serialization must succeed");

        // Hard security assertions:
        assert!(
            !json.contains(sensitive_window_title),
            "DumpState JSON must NEVER contain window titles"
        );
        assert!(
            !json.contains("Secret Bank Statement"),
            "DumpState JSON must NEVER contain sensitive window title substring"
        );
        assert!(
            !json.contains(sensitive_notif_body),
            "DumpState JSON must NEVER contain notification bodies"
        );
        assert!(
            !json.contains("987654"),
            "DumpState JSON must NEVER contain 2FA or secret tokens"
        );
        assert!(
            !json.contains("\"title\""),
            "DumpState schema must not contain a 'title' field"
        );
        assert!(
            !json.contains("\"body\""),
            "DumpState schema must not contain a 'body' field"
        );
        assert!(
            !json.contains("\"summary\""),
            "DumpState schema must not contain a 'summary' field"
        );

        // Required observability fields:
        assert!(json.contains("\"org.mozilla.firefox\""));
        assert!(json.contains("\"{1111-2222-3333-4444}\""));
        assert!(json.contains("\"models_ready\": true"));
        assert!(json.contains("\"daemon_connected\": true"));
        assert!(json.contains("\"entry_count\": 1"));
        assert!(json.contains("\"active_count\": 1"));

        // Round-trip verification:
        let parsed: ShellDebugState =
            serde_json::from_str(&json).expect("Deserialization must succeed");
        assert_eq!(parsed, state);
    }
}
