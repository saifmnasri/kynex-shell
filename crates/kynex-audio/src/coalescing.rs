//! Audio change event coalescing to prevent IPC flood and redundant re-queries.

use crate::limits::{DEFAULT_COALESCE_DEBOUNCE_MS, MAX_COALESCE_WINDOW_MS, QUIET_RESET_MS};

/// Action returned by [`EventCoalescer`] when processing an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoalesceAction {
    /// Schedule a debounce timer for the specified delay in milliseconds.
    ScheduleDebounce(u64),
    /// An update is already scheduled; event is coalesced into the pending run.
    AlreadyScheduled,
}

/// Pure state machine for coalescing rapid bursts of audio events into bounded refreshes.
///
/// Implements adaptive coalescing:
/// - Base coalescing window (25 ms)
/// - Exponential window widening (up to 250 ms) while events arrive faster than the window
/// - Quiet period reset (resets to base window after 1000 ms of inactivity)
/// - Final-state delivery guarantee (dirty state tracks pending changes to ensure the trailing edge is never dropped)
#[derive(Debug, Clone)]
pub struct EventCoalescer {
    min_window_ms: u64,
    max_window_ms: u64,
    quiet_reset_ms: u64,
    current_window_ms: u64,
    last_event_ms: Option<u64>,
    scheduled: bool,
    dirty: bool,
}

impl Default for EventCoalescer {
    fn default() -> Self {
        Self::new(DEFAULT_COALESCE_DEBOUNCE_MS)
    }
}

impl EventCoalescer {
    /// Creates a new coalescer with the specified base window interval in milliseconds.
    #[must_use]
    pub fn new(min_window_ms: u64) -> Self {
        Self::with_limits(min_window_ms, MAX_COALESCE_WINDOW_MS, QUIET_RESET_MS)
    }

    /// Creates a new coalescer with explicit window bounds and quiet reset duration.
    #[must_use]
    pub fn with_limits(min_window_ms: u64, max_window_ms: u64, quiet_reset_ms: u64) -> Self {
        Self {
            min_window_ms,
            max_window_ms,
            quiet_reset_ms,
            current_window_ms: min_window_ms,
            last_event_ms: None,
            scheduled: false,
            dirty: false,
        }
    }

    /// Handles an incoming event at the given monotonic timestamp `now_ms`.
    pub fn handle_event(&mut self, now_ms: u64) -> CoalesceAction {
        if let Some(last) = self.last_event_ms {
            let elapsed = now_ms.saturating_sub(last);
            if elapsed >= self.quiet_reset_ms {
                // Reset window after quiet period
                self.current_window_ms = self.min_window_ms;
            } else if elapsed < self.current_window_ms {
                // Events arriving faster than current window: widen exponentially up to max_window_ms
                self.current_window_ms = self
                    .current_window_ms
                    .saturating_mul(2)
                    .min(self.max_window_ms);
            }
        }
        self.last_event_ms = Some(now_ms);

        if self.scheduled {
            self.dirty = true;
            CoalesceAction::AlreadyScheduled
        } else {
            self.scheduled = true;
            self.dirty = false;
            CoalesceAction::ScheduleDebounce(self.current_window_ms)
        }
    }

    /// Marks the debounced query run as completed at timestamp `now_ms`.
    ///
    /// If dirty events arrived while scheduled, schedules a trailing debounce with
    /// the current (widened) window and returns `Some(delay_ms)`, ensuring the final state
    /// is delivered. Otherwise returns `None` and marks the coalescer as idle.
    pub fn on_debounced_run(&mut self, _now_ms: u64) -> Option<u64> {
        if self.dirty {
            self.dirty = false;
            self.scheduled = true;
            Some(self.current_window_ms)
        } else {
            self.scheduled = false;
            None
        }
    }

    /// Resets the coalescer state back to initial idle defaults.
    pub fn reset(&mut self) {
        self.current_window_ms = self.min_window_ms;
        self.last_event_ms = None;
        self.scheduled = false;
        self.dirty = false;
    }

    /// Current active coalescing window duration in milliseconds.
    #[must_use]
    pub fn current_window_ms(&self) -> u64 {
        self.current_window_ms
    }

    /// Minimum base window duration in milliseconds.
    #[must_use]
    pub fn min_window_ms(&self) -> u64 {
        self.min_window_ms
    }

    /// Maximum window cap duration in milliseconds.
    #[must_use]
    pub fn max_window_ms(&self) -> u64 {
        self.max_window_ms
    }

    /// Quiet period duration in milliseconds required to reset window.
    #[must_use]
    pub fn quiet_reset_ms(&self) -> u64 {
        self.quiet_reset_ms
    }

    /// Whether a debounce timer is currently scheduled.
    #[must_use]
    pub fn is_scheduled(&self) -> bool {
        self.scheduled
    }

    /// Whether new events have arrived while a debounce run was scheduled.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Monotonic timestamp of the most recent event, if any.
    #[must_use]
    pub fn last_event_ms(&self) -> Option<u64> {
        self.last_event_ms
    }
}
