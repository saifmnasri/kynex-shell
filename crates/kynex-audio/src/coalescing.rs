//! Audio change event coalescing to prevent IPC flood and redundant re-queries.

use crate::limits::DEFAULT_COALESCE_DEBOUNCE_MS;

/// Action returned by [`EventCoalescer`] when processing an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoalesceAction {
    /// No recent event; caller should trigger an immediate refresh.
    TriggerImmediate,
    /// An event arrived while within debounce interval; caller should debounce with the given delay.
    ScheduleDebounce(u64),
    /// A debounced refresh is already scheduled; event is coalesced into that pending run.
    AlreadyScheduled,
}

/// Pure state machine for coalescing rapid bursts of audio events into bounded refreshes.
#[derive(Debug, Clone)]
pub struct EventCoalescer {
    debounce_ms: u64,
    last_trigger_ms: Option<u64>,
    scheduled: bool,
}

impl Default for EventCoalescer {
    fn default() -> Self {
        Self::new(DEFAULT_COALESCE_DEBOUNCE_MS)
    }
}

impl EventCoalescer {
    /// Creates a new coalescer with the specified debounce interval in milliseconds.
    #[must_use]
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            last_trigger_ms: None,
            scheduled: false,
        }
    }

    /// Handles an incoming event at the given monotonic timestamp `now_ms`.
    pub fn handle_event(&mut self, now_ms: u64) -> CoalesceAction {
        if self.scheduled {
            return CoalesceAction::AlreadyScheduled;
        }

        match self.last_trigger_ms {
            None => {
                self.last_trigger_ms = Some(now_ms);
                CoalesceAction::TriggerImmediate
            }
            Some(last) => {
                let elapsed = now_ms.saturating_sub(last);
                if elapsed >= self.debounce_ms {
                    self.last_trigger_ms = Some(now_ms);
                    CoalesceAction::TriggerImmediate
                } else {
                    self.scheduled = true;
                    let remaining = self.debounce_ms.saturating_sub(elapsed);
                    CoalesceAction::ScheduleDebounce(remaining)
                }
            }
        }
    }

    /// Marks the scheduled debounce action as completed at timestamp `now_ms`.
    pub fn on_debounced_run(&mut self, now_ms: u64) {
        self.scheduled = false;
        self.last_trigger_ms = Some(now_ms);
    }

    /// Resets the coalescer state.
    pub fn reset(&mut self) {
        self.last_trigger_ms = None;
        self.scheduled = false;
    }
}
