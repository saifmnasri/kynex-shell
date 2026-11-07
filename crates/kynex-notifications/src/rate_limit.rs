//! Token-bucket rate limiting per D-Bus connection sender.

use std::collections::HashMap;
use std::time::Instant;

use crate::error::NotificationError;
use crate::limits::{
    RATE_LIMIT_BURST, RATE_LIMIT_MAX_TRACKED_SENDERS, RATE_LIMIT_SUSTAINED_PER_SEC,
};

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: Instant,
}

impl TokenBucket {
    fn new(now: Instant) -> Self {
        Self {
            tokens: RATE_LIMIT_BURST,
            last_update: now,
        }
    }

    fn try_consume(&mut self, now: Instant) -> bool {
        let elapsed = now
            .saturating_duration_since(self.last_update)
            .as_secs_f64();
        self.last_update = now;

        // Refill tokens according to sustained rate, capped at burst capacity
        self.tokens = (self.tokens + elapsed * RATE_LIMIT_SUSTAINED_PER_SEC).min(RATE_LIMIT_BURST);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Token-bucket rate limiter tracking D-Bus senders.
#[derive(Debug, Default)]
pub struct RateLimiter {
    buckets: HashMap<String, TokenBucket>,
}

impl RateLimiter {
    /// Creates an empty rate limiter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buckets: HashMap::new(),
        }
    }

    /// Evaluates rate limit for `sender` at timestamp `now`.
    ///
    /// Consumes 1 token if permitted, or returns `NotificationError::LimitsExceeded`.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationError::LimitsExceeded`] if sender burst/sustained limit
    /// or global tracked senders cap is exceeded.
    pub fn check_and_consume(
        &mut self,
        sender: &str,
        now: Instant,
    ) -> Result<(), NotificationError> {
        // Enforce cleanup if tracking table grows large
        if self.buckets.len() >= RATE_LIMIT_MAX_TRACKED_SENDERS
            && !self.buckets.contains_key(sender)
        {
            // Prune buckets that have been full and untouched for > 60 seconds
            self.buckets.retain(|_, bucket| {
                let inactive_sec = now
                    .saturating_duration_since(bucket.last_update)
                    .as_secs_f64();
                inactive_sec < 60.0
            });

            if self.buckets.len() >= RATE_LIMIT_MAX_TRACKED_SENDERS {
                return Err(NotificationError::LimitsExceeded(
                    "Global concurrent sender limit exceeded",
                ));
            }
        }

        let bucket = self
            .buckets
            .entry(sender.to_string())
            .or_insert_with(|| TokenBucket::new(now));

        if bucket.try_consume(now) {
            Ok(())
        } else {
            Err(NotificationError::LimitsExceeded(
                "Per-sender notification rate limit exceeded (burst 10, 2/s sustained)",
            ))
        }
    }

    /// Cleans up state when a sender disconnects.
    pub fn remove_sender(&mut self, sender: &str) {
        self.buckets.remove(sender);
    }
}
