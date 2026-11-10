//! Comprehensive tests for `TaskbarCore` (M2.3) and Launcher pin toggle (M2.4).

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use kynex_ipc::apps::AppItem;
use kynex_ipc::windows::WindowItem;
use kynex_shell_core::launcher::LauncherCore;
use kynex_shell_core::matching::AppCandidate;
use kynex_shell_core::taskbar::{TaskbarAction, TaskbarCore};

#[test]
fn test_taskbar_pinned_and_running_ordering() {
    let mut core = TaskbarCore::new();

    let candidates = vec![
        AppCandidate::new(
            "org.gnome.Terminal.desktop",
            Some("gnome-terminal-server"),
            "org.gnome.Terminal",
        ),
        AppCandidate::new("firefox.desktop", None::<String>, "firefox"),
        AppCandidate::new(
            "org.gnome.Nautilus.desktop",
            None::<String>,
            "system-file-manager",
        ),
    ];
    let mut names = HashMap::new();
    names.insert(
        "org.gnome.Terminal.desktop".to_string(),
        "Terminal".to_string(),
    );
    names.insert("firefox.desktop".to_string(), "Firefox".to_string());
    names.insert(
        "org.gnome.Nautilus.desktop".to_string(),
        "Files".to_string(),
    );

    core.set_candidates(candidates, names);
    core.set_pinned_ids(vec![
        "firefox.desktop".to_string(),
        "org.gnome.Terminal.desktop".to_string(),
    ]);

    // Currently only Nautilus (unpinned) and Terminal (pinned) are running
    let windows = vec![
        WindowItem {
            id: "win-term-1".to_string(),
            title: "Terminal 1".to_string(),
            app_id: "gnome-terminal-server".to_string(),
            minimized: false,
            active: true,
        },
        WindowItem {
            id: "win-files-1".to_string(),
            title: "Home".to_string(),
            app_id: "org.gnome.Nautilus".to_string(),
            minimized: false,
            active: false,
        },
    ];
    core.set_windows(windows);

    let entries = core.entries();
    assert_eq!(
        entries.len(),
        3,
        "Must have 2 pinned apps + 1 unpinned running app"
    );

    // Index 0: firefox (pinned, NOT running)
    assert_eq!(entries[0].app_id, "firefox.desktop");
    assert_eq!(entries[0].name, "Firefox");
    assert!(entries[0].is_pinned);
    assert!(!entries[0].is_running);
    assert_eq!(entries[0].window_count, 0);

    // Index 1: terminal (pinned, running, active)
    assert_eq!(entries[1].app_id, "org.gnome.Terminal.desktop");
    assert_eq!(entries[1].name, "Terminal");
    assert!(entries[1].is_pinned);
    assert!(entries[1].is_running);
    assert!(entries[1].is_active);
    assert_eq!(entries[1].window_count, 1);
    assert_eq!(entries[1].window_ids, vec!["win-term-1"]);

    // Index 2: nautilus (unpinned, running, inactive)
    assert_eq!(entries[2].app_id, "org.gnome.Nautilus.desktop");
    assert_eq!(entries[2].name, "Files");
    assert!(!entries[2].is_pinned);
    assert!(entries[2].is_running);
    assert!(!entries[2].is_active);
    assert_eq!(entries[2].window_count, 1);
}

#[test]
fn test_taskbar_click_interactions_and_multi_window_cycling() {
    let mut core = TaskbarCore::new();

    let candidates = vec![
        AppCandidate::new("firefox.desktop", None::<String>, "firefox"),
        AppCandidate::new(
            "org.gnome.Terminal.desktop",
            Some("gnome-terminal-server"),
            "org.gnome.Terminal",
        ),
    ];
    core.set_candidates(candidates, HashMap::new());
    core.set_pinned_ids(vec!["firefox.desktop".to_string()]);

    // 1. Click on pinned, not running app -> Launch
    let action0 = core.handle_click(0);
    assert_eq!(
        action0,
        TaskbarAction::Launch {
            app_id: "firefox.desktop".to_string()
        }
    );

    // 2. Open 1 window for firefox (not active) -> Activate
    core.set_windows(vec![WindowItem {
        id: "win-fox-1".to_string(),
        title: "Mozilla Firefox".to_string(),
        app_id: "firefox".to_string(),
        minimized: false,
        active: false,
    }]);
    let action_activate = core.handle_click(0);
    assert_eq!(
        action_activate,
        TaskbarAction::Activate {
            window_id: "win-fox-1".to_string()
        }
    );

    // 3. Window is active -> Minimize toggle
    core.set_windows(vec![WindowItem {
        id: "win-fox-1".to_string(),
        title: "Mozilla Firefox".to_string(),
        app_id: "firefox".to_string(),
        minimized: false,
        active: true,
    }]);
    let action_minimize = core.handle_click(0);
    assert_eq!(
        action_minimize,
        TaskbarAction::Minimize {
            window_id: "win-fox-1".to_string()
        }
    );

    // 4. Multiple windows for terminal (win-1 active, win-2 inactive) -> Cycle to win-2
    core.set_windows(vec![
        WindowItem {
            id: "win-term-1".to_string(),
            title: "bash 1".to_string(),
            app_id: "gnome-terminal-server".to_string(),
            minimized: false,
            active: true,
        },
        WindowItem {
            id: "win-term-2".to_string(),
            title: "bash 2".to_string(),
            app_id: "gnome-terminal-server".to_string(),
            minimized: false,
            active: false,
        },
    ]);
    // terminal is at index 1 (unpinned running)
    let action_cycle1 = core.handle_click(1);
    assert_eq!(
        action_cycle1,
        TaskbarAction::Activate {
            window_id: "win-term-2".to_string()
        }
    );

    // 5. win-2 active -> Cycle wrap-around to win-1
    core.set_windows(vec![
        WindowItem {
            id: "win-term-1".to_string(),
            title: "bash 1".to_string(),
            app_id: "gnome-terminal-server".to_string(),
            minimized: false,
            active: false,
        },
        WindowItem {
            id: "win-term-2".to_string(),
            title: "bash 2".to_string(),
            app_id: "gnome-terminal-server".to_string(),
            minimized: false,
            active: true,
        },
    ]);
    let action_cycle2 = core.handle_click(1);
    assert_eq!(
        action_cycle2,
        TaskbarAction::Activate {
            window_id: "win-term-1".to_string()
        }
    );
}

#[test]
fn test_taskbar_middle_click_close() {
    let mut core = TaskbarCore::new();
    core.set_pinned_ids(vec!["firefox.desktop".to_string()]);

    // Not running: middle click does nothing
    assert_eq!(core.handle_middle_click(0), TaskbarAction::None);

    // Running single window: closes that window
    core.set_windows(vec![WindowItem {
        id: "win-fox-1".to_string(),
        title: "Firefox".to_string(),
        app_id: "firefox.desktop".to_string(),
        minimized: false,
        active: false,
    }]);
    assert_eq!(
        core.handle_middle_click(0),
        TaskbarAction::Close {
            window_id: "win-fox-1".to_string()
        }
    );

    // Running multi-window: closes the active one
    core.set_windows(vec![
        WindowItem {
            id: "win-fox-1".to_string(),
            title: "Firefox 1".to_string(),
            app_id: "firefox.desktop".to_string(),
            minimized: false,
            active: false,
        },
        WindowItem {
            id: "win-fox-2".to_string(),
            title: "Firefox 2".to_string(),
            app_id: "firefox.desktop".to_string(),
            minimized: false,
            active: true,
        },
    ]);
    assert_eq!(
        core.handle_middle_click(0),
        TaskbarAction::Close {
            window_id: "win-fox-2".to_string()
        }
    );
}

#[test]
fn test_taskbar_edge_cases_and_coverage() {
    let mut core = TaskbarCore::new();

    // Out of bounds click/middle click
    assert_eq!(core.handle_click(99), TaskbarAction::None);
    assert_eq!(core.handle_middle_click(99), TaskbarAction::None);

    // Multi-window where none of the windows is active
    let candidates = vec![AppCandidate::new(
        "org.gnome.Terminal.desktop",
        Some("gnome-terminal-server"),
        "org.gnome.Terminal",
    )];
    core.set_candidates(candidates, HashMap::new());
    core.set_windows(vec![
        WindowItem {
            id: "win-1".to_string(),
            title: "term 1".to_string(),
            app_id: "gnome-terminal-server".to_string(),
            minimized: false,
            active: false,
        },
        WindowItem {
            id: "win-2".to_string(),
            title: "term 2".to_string(),
            app_id: "gnome-terminal-server".to_string(),
            minimized: false,
            active: false,
        },
    ]);

    // When none is active, handle_click activates the first window
    let act = core.handle_click(0);
    assert_eq!(
        act,
        TaskbarAction::Activate {
            window_id: "win-1".to_string()
        }
    );

    // When none is active, middle-click closes the last window
    let close = core.handle_middle_click(0);
    assert_eq!(
        close,
        TaskbarAction::Close {
            window_id: "win-2".to_string()
        }
    );
}

#[test]
fn test_launcher_core_methods_coverage() {
    let mut launcher = LauncherCore::new();
    assert_eq!(launcher.count(), 0);
    assert_eq!(launcher.selected_index(), -1);
    assert_eq!(launcher.query(), "");
    assert_eq!(launcher.items().len(), 0);

    launcher.set_selected_index(5);
    assert_eq!(launcher.selected_index(), -1);

    let (seq, q) = launcher.prepare_search_query("test");
    assert_eq!(seq, 1);
    assert_eq!(q, "test");
    assert_eq!(launcher.current_seq(), 1);

    let raw_apps = vec![AppItem {
        id: "app1.desktop".to_string(),
        name: "App One".to_string(),
        generic_name: String::new(),
        comment: String::new(),
        icon_name: "app1".to_string(),
        categories: vec![],
    }];
    launcher.apply_results(1, &raw_apps);
    assert_eq!(launcher.count(), 1);
    assert_eq!(launcher.selected_index(), 0);

    launcher.set_selected_index(0);
    assert_eq!(launcher.selected_index(), 0);
    launcher.set_selected_index(99);
    assert_eq!(launcher.selected_index(), -1);

    // Toggle pin with no selection returns None
    assert_eq!(launcher.toggle_pin_selected(), None);
}

#[test]
fn test_launcher_ctrl_p_pin_toggle() {
    let mut launcher = LauncherCore::new();
    launcher.set_pinned_ids(vec!["pinned.app.desktop".to_string()]);

    let raw_apps = vec![
        AppItem {
            id: "pinned.app.desktop".to_string(),
            name: "Pinned App".to_string(),
            generic_name: String::new(),
            comment: String::new(),
            icon_name: "pinned-app".to_string(),
            categories: vec![],
        },
        AppItem {
            id: "other.app.desktop".to_string(),
            name: "Other App".to_string(),
            generic_name: String::new(),
            comment: String::new(),
            icon_name: "other-app".to_string(),
            categories: vec![],
        },
    ];

    launcher.apply_results(1, &raw_apps);
    assert_eq!(launcher.count(), 2);
    assert_eq!(launcher.selected_index(), 0);

    // Initial item 0 is pinned
    assert!(launcher.items()[0].is_pinned);
    // Initial item 1 is not pinned
    assert!(!launcher.items()[1].is_pinned);

    // Toggle item 0 -> unpin
    let unpinned = launcher.toggle_pin_selected();
    assert_eq!(unpinned, Some(("pinned.app.desktop".to_string(), false)));
    assert!(!launcher.items()[0].is_pinned);

    // Select item 1 and toggle -> pin
    launcher.select_next();
    assert_eq!(launcher.selected_index(), 1);
    let pinned = launcher.toggle_pin_selected();
    assert_eq!(pinned, Some(("other.app.desktop".to_string(), true)));
    assert!(launcher.items()[1].is_pinned);
}
