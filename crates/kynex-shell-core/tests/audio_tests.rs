//! Unit tests for pure-Rust shell core audio state, placement, and slider debouncing.

use kynex_shell_core::{
    compute_quick_settings_placement, AudioState, DebounceAction, OutputGeometry, PanelSize, Point,
    SliderDebouncer, SurfaceAnchors,
};

#[test]
fn test_audio_state_from_raw() {
    let state = AudioState::from_raw(true, 75, false, "Speakers");
    assert!(state.present);
    assert_eq!(state.volume, 75);
    assert!(!state.muted);
    assert_eq!(state.device_name, "Speakers");

    // Clamping volume
    let clamped = AudioState::from_raw(true, 150, true, "Headphones");
    assert_eq!(clamped.volume, 100);
}

#[test]
fn test_slider_debouncer_immediate_send() {
    let mut debouncer = SliderDebouncer::new(50);

    // Initial input at t=100ms
    assert_eq!(
        debouncer.handle_input(40, 100),
        DebounceAction::SendImmediate(40)
    );
    assert_eq!(debouncer.pending_value(), None);
    assert!(!debouncer.is_timer_active());

    // Next input after 60ms (> 50ms interval) -> immediate send
    assert_eq!(
        debouncer.handle_input(50, 160),
        DebounceAction::SendImmediate(50)
    );
    assert_eq!(debouncer.pending_value(), None);
    assert!(!debouncer.is_timer_active());
}

#[test]
fn test_slider_debouncer_coalesces_rapid_burst_and_sends_final_value() {
    let mut debouncer = SliderDebouncer::new(50);

    // Initial write at t=100ms
    assert_eq!(
        debouncer.handle_input(10, 100),
        DebounceAction::SendImmediate(10)
    );

    // Rapid drag at t=105ms -> schedules timer with remaining 45ms delay
    assert_eq!(
        debouncer.handle_input(20, 105),
        DebounceAction::ScheduleTimer {
            delay_ms: 45,
            value: 20
        }
    );
    assert!(debouncer.is_timer_active());
    assert_eq!(debouncer.pending_value(), Some(20));

    // Rapid successive drag inputs at t=110, 120, 130, 140, 145ms
    assert_eq!(
        debouncer.handle_input(35, 110),
        DebounceAction::UpdatePending(35)
    );
    assert_eq!(
        debouncer.handle_input(50, 120),
        DebounceAction::UpdatePending(50)
    );
    assert_eq!(
        debouncer.handle_input(75, 130),
        DebounceAction::UpdatePending(75)
    );
    assert_eq!(
        debouncer.handle_input(90, 140),
        DebounceAction::UpdatePending(90)
    );
    assert_eq!(
        debouncer.handle_input(100, 145),
        DebounceAction::UpdatePending(100)
    );

    // Ensure pending value tracks the latest final value
    assert_eq!(debouncer.pending_value(), Some(100));

    // Timer fires at t=150ms -> MUST yield the FINAL value (100)
    let final_sent = debouncer.on_timer(150);
    assert_eq!(final_sent, Some(100));
    assert_eq!(debouncer.pending_value(), None);
    assert!(!debouncer.is_timer_active());

    // If timer fires again with no pending value, returns None
    assert_eq!(debouncer.on_timer(155), None);
}

#[test]
fn test_slider_debouncer_reset() {
    let mut debouncer = SliderDebouncer::new(50);
    let _ = debouncer.handle_input(10, 100);
    let _ = debouncer.handle_input(20, 110);
    assert!(debouncer.is_timer_active());

    debouncer.reset();
    assert!(!debouncer.is_timer_active());
    assert_eq!(debouncer.pending_value(), None);

    // After reset, next write is immediate
    assert_eq!(
        debouncer.handle_input(30, 115),
        DebounceAction::SendImmediate(30)
    );
}

#[test]
fn test_quick_settings_placement_bottom_panel_opens_above() {
    let anchors = SurfaceAnchors {
        top: false,
        bottom: true,
        left: true,
        right: true,
    };
    let panel = PanelSize {
        width: 1920.0,
        height: 48.0,
    };
    let anchor_local = Point::new(1800.0, 4.0); // Status button on right of panel
    let anchor_width = 100.0;
    let anchor_height = 40.0;
    let popup_width = 320.0;
    let popup_height = 240.0;
    let screen = OutputGeometry {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
        scale: 1.0,
    };

    let pos = compute_quick_settings_placement(
        anchors,
        panel,
        anchor_local,
        anchor_width,
        anchor_height,
        popup_width,
        popup_height,
        screen,
    );

    // Panel is at bottom: panel_y = 1080 - 48 = 1032
    // anchor_y = 1032 + 4 = 1036
    // Opens above panel: y = 1036 - 240 = 796
    assert_eq!(pos.y, 796.0);

    // Anchor x is 1800, width 100, popup width 320.
    // 1800 + 320 = 2120 > 1920 screen width, so flips horizontally:
    // x = anchor_x + anchor_width - popup_width = 1800 + 100 - 320 = 1580
    assert_eq!(pos.x, 1580.0);
}

#[test]
fn test_quick_settings_placement_negative_origin_output_local_contract() {
    // Secondary monitor positioned at global compositor coordinate x = -1920 (left of primary display).
    // In Wayland LayerShell, popup coordinates must be OUTPUT-LOCAL [0, 1920], NOT global [-1920, 0].
    let negative_origin_screen = OutputGeometry {
        x: -1920.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
        scale: 1.0,
    };
    let anchors = SurfaceAnchors {
        top: false,
        bottom: true,
        left: true,
        right: true,
    };
    let panel = PanelSize {
        width: 1920.0,
        height: 48.0,
    };
    let anchor_local = Point::new(1700.0, 4.0);
    let popup_width = 360.0;
    let popup_height = 420.0;

    let pos = compute_quick_settings_placement(
        anchors,
        panel,
        anchor_local,
        200.0,
        40.0,
        popup_width,
        popup_height,
        negative_origin_screen,
    );

    // Position MUST be strictly output-local (landing within [0, 1920] and [0, 1080]), NOT offset by -1920
    assert!(
        pos.x >= 0.0 && pos.x + popup_width <= 1920.0,
        "Output-local popup x ({}) + width ({}) must stay inside output boundary [0, 1920]",
        pos.x,
        popup_width
    );
    assert!(
        pos.y >= 0.0 && pos.y + popup_height <= 1080.0,
        "Output-local popup y ({}) + height ({}) must stay inside output boundary [0, 1080]",
        pos.y,
        popup_height
    );
    assert_eq!(
        pos.x, 1540.0,
        "Popup horizontal flip stays in output bounds"
    );
    assert_eq!(
        pos.y, 616.0,
        "Popup vertical positioning above bottom panel stays in output bounds"
    );
}

#[test]
fn test_mutant_debounce_final_value() {
    // Mutant: debouncer drops trailing value -> final slider position lost.
    let mut debouncer = SliderDebouncer::new(50);
    assert_eq!(
        debouncer.handle_input(10, 100),
        DebounceAction::SendImmediate(10)
    );
    let _ = debouncer.handle_input(20, 110);
    let _ = debouncer.handle_input(30, 120);
    let _ = debouncer.handle_input(40, 130);
    let _ = debouncer.handle_input(99, 140);

    assert_eq!(
        debouncer.on_timer(150),
        Some(99),
        "Mutant killed: Debouncer guarantees final trailing value is emitted"
    );
}
