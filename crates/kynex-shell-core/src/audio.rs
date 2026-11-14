//! Pure-Rust business logic for shell audio presentation and slider debouncing.
//!
//! Provides slider rate limiting guaranteeing <= 20 IPC writes/second while ensuring
//! the final settled slider value is always transmitted to the backend daemon.

use crate::coordinates::{OutputGeometry, PanelSize, Point, SurfaceAnchors};
use crate::menu::compute_menu_placement;

/// Action yielded by [`SliderDebouncer::handle_input`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceAction {
    /// No recent write; dispatch value immediately.
    SendImmediate(u32),
    /// Cooldown active; schedule a timer with the given delay in milliseconds.
    ScheduleTimer {
        /// Delay until timer should fire.
        delay_ms: u64,
        /// The value that will be dispatched if no newer input arrives.
        value: u32,
    },
    /// Timer is already running; pending value updated to the latest value.
    UpdatePending(u32),
}

/// Pure state machine enforcing slider write debouncing up to 20/s (>= 50ms interval).
///
/// Invariant: The final user input value is GUARANTEED to be dispatched, even during
/// rapid continuous dragging.
#[derive(Debug, Clone)]
pub struct SliderDebouncer {
    min_interval_ms: u64,
    last_sent_ms: Option<u64>,
    pending_value: Option<u32>,
    timer_active: bool,
}

impl Default for SliderDebouncer {
    fn default() -> Self {
        Self::new(50) // 50 ms = 20 writes/second
    }
}

impl SliderDebouncer {
    /// Creates a debouncer with the specified minimum interval in milliseconds.
    #[must_use]
    pub fn new(min_interval_ms: u64) -> Self {
        Self {
            min_interval_ms,
            last_sent_ms: None,
            pending_value: None,
            timer_active: false,
        }
    }

    /// Processes a new user slider input value at monotonic timestamp `now_ms`.
    pub fn handle_input(&mut self, value: u32, now_ms: u64) -> DebounceAction {
        let clamped = value.clamp(0, 100);

        if self.timer_active {
            self.pending_value = Some(clamped);
            return DebounceAction::UpdatePending(clamped);
        }

        match self.last_sent_ms {
            None => {
                self.last_sent_ms = Some(now_ms);
                self.pending_value = None;
                DebounceAction::SendImmediate(clamped)
            }
            Some(last) => {
                let elapsed = now_ms.saturating_sub(last);
                if elapsed >= self.min_interval_ms {
                    self.last_sent_ms = Some(now_ms);
                    self.pending_value = None;
                    DebounceAction::SendImmediate(clamped)
                } else {
                    self.timer_active = true;
                    self.pending_value = Some(clamped);
                    let delay = self.min_interval_ms.saturating_sub(elapsed);
                    DebounceAction::ScheduleTimer {
                        delay_ms: delay,
                        value: clamped,
                    }
                }
            }
        }
    }

    /// Invoked when the debounce timer fires at monotonic timestamp `now_ms`.
    ///
    /// Returns the final settled value to dispatch, or `None` if no pending value was queued.
    pub fn on_timer(&mut self, now_ms: u64) -> Option<u32> {
        self.timer_active = false;
        if let Some(final_value) = self.pending_value.take() {
            self.last_sent_ms = Some(now_ms);
            Some(final_value)
        } else {
            None
        }
    }

    /// Returns the currently queued pending value, if any.
    #[must_use]
    pub fn pending_value(&self) -> Option<u32> {
        self.pending_value
    }

    /// Returns whether a debounce timer is currently active.
    #[must_use]
    pub fn is_timer_active(&self) -> bool {
        self.timer_active
    }

    /// Resets all debouncer state.
    pub fn reset(&mut self) {
        self.last_sent_ms = None;
        self.pending_value = None;
        self.timer_active = false;
    }
}

/// Pure presentation state for shell audio models.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioState {
    /// Whether audio hardware / backend is detected and active.
    pub present: bool,
    /// Master volume level clamped between 0 and 100.
    pub volume: u32,
    /// Whether audio output is muted.
    pub muted: bool,
    /// Sanitized name of the active audio output device.
    pub device_name: String,
}

impl AudioState {
    /// Constructs a validated `AudioState` from raw D-Bus values.
    #[must_use]
    pub fn from_raw(present: bool, raw_volume: u32, muted: bool, device_name: &str) -> Self {
        Self {
            present,
            volume: raw_volume.clamp(0, 100),
            muted,
            device_name: device_name.to_string(),
        }
    }
}

/// Computes the clamped placement for the Quick Settings overlay popup.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn compute_quick_settings_placement(
    anchors: SurfaceAnchors,
    panel: PanelSize,
    anchor_local: Point,
    anchor_width: f64,
    anchor_height: f64,
    popup_width: f64,
    popup_height: f64,
    screen: OutputGeometry,
) -> Point {
    compute_menu_placement(
        anchors,
        panel,
        anchor_local,
        anchor_width,
        anchor_height,
        popup_width,
        popup_height,
        screen,
    )
}
