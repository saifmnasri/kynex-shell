//! Token-bucket rate limiter for tray item property refresh signals.

use std::collections::HashMap;
use std::time::Instant;

use crate::error::TrayError;
use crate::limits::MAX_REFRESHES_PER_SEC;

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: Instant,
}

impl TokenBucket {
    fn new(now: Instant) -> Self {
        Self {
            tokens: f64::from(MAX_REFRESHES_PER_SEC),
            last_update: now,
        }
    }

    fn try_consume(&mut self, now: Instant) -> bool {
        let elapsed = now
            .saturating_duration_since(self.last_update)
            .as_secs_f64();
        self.last_update = now;

        let max_tokens = f64::from(MAX_REFRESHES_PER_SEC);
        self.tokens = (self.tokens + elapsed * max_tokens).min(max_tokens);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Token-bucket rate limiter tracking item refresh signals.
#[derive(Debug, Default)]
pub struct RefreshRateLimiter {
    buckets: HashMap<String, TokenBucket>,
}

impl RefreshRateLimiter {
    /// Creates a new empty rate limiter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buckets: HashMap::new(),
        }
    }

    /// Evaluates rate limit for item `item_id` at timestamp `now`.
    ///
    /// Consumes 1 token if permitted, or returns [`TrayError::RateLimitExceeded`].
    ///
    /// # Errors
    ///
    /// Returns [`TrayError::RateLimitExceeded`] if refresh rate exceeds [`MAX_REFRESHES_PER_SEC`].
    pub fn check_and_consume(&mut self, item_id: &str, now: Instant) -> Result<(), TrayError> {
        let bucket = self
            .buckets
            .entry(item_id.to_string())
            .or_insert_with(|| TokenBucket::new(now));

        if bucket.try_consume(now) {
            Ok(())
        } else {
            Err(TrayError::RateLimitExceeded)
        }
    }

    /// Removes tracking state for `item_id`.
    pub fn remove(&mut self, item_id: &str) {
        self.buckets.remove(item_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_rate_limiter_burst_and_refill() {
        let mut limiter = RefreshRateLimiter::new();
        let now = Instant::now();

        // Should allow MAX_REFRESHES_PER_SEC (10) consumes immediately
        for _ in 0..MAX_REFRESHES_PER_SEC {
            assert!(limiter.check_and_consume("item-1", now).is_ok());
        }

        // 11th call immediately fails
        assert_eq!(
            limiter.check_and_consume("item-1", now),
            Err(TrayError::RateLimitExceeded)
        );

        // After 100ms (0.1s), 1 token refilled (10 * 0.1 = 1)
        let later = now + Duration::from_millis(150);
        assert!(limiter.check_and_consume("item-1", later).is_ok());

        // Clean removal
        limiter.remove("item-1");
    }
}
