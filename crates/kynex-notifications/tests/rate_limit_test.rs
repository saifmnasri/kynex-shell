//! Token-bucket rate limiting unit tests.
#![allow(
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::time::{Duration, Instant};

use kynex_notifications::{NotificationError, RateLimiter, RATE_LIMIT_BURST};

#[test]
fn test_burst_capacity_and_exhaustion() {
    let mut limiter = RateLimiter::new();
    let now = Instant::now();
    let sender = ":1.123";

    // First RATE_LIMIT_BURST (10) calls in the exact same millisecond succeed
    for _ in 0..(RATE_LIMIT_BURST as usize) {
        assert!(limiter.check_and_consume(sender, now).is_ok());
    }

    // 11th call without time advancement must fail with LimitsExceeded
    let err = limiter.check_and_consume(sender, now);
    assert_eq!(
        err,
        Err(NotificationError::LimitsExceeded(
            "Per-sender notification rate limit exceeded (burst 10, 2/s sustained)"
        ))
    );
}

#[test]
fn test_sustained_refill_rate() {
    let mut limiter = RateLimiter::new();
    let start = Instant::now();
    let sender = ":1.456";

    // Exhaust burst capacity
    for _ in 0..(RATE_LIMIT_BURST as usize) {
        let _ = limiter.check_and_consume(sender, start);
    }
    assert!(limiter.check_and_consume(sender, start).is_err());

    // Advance 500 ms (refills 1 token at 2 tokens/sec)
    let t_500ms = start + Duration::from_millis(500);
    assert!(limiter.check_and_consume(sender, t_500ms).is_ok());
    assert!(limiter.check_and_consume(sender, t_500ms).is_err());

    // Advance 1000 ms (refills 2 tokens)
    let t_1500ms = start + Duration::from_millis(1500);
    assert!(limiter.check_and_consume(sender, t_1500ms).is_ok());
    assert!(limiter.check_and_consume(sender, t_1500ms).is_ok());
    assert!(limiter.check_and_consume(sender, t_1500ms).is_err());
}

#[test]
fn test_independent_per_sender_limits() {
    let mut limiter = RateLimiter::new();
    let now = Instant::now();

    // Exhaust sender A
    for _ in 0..(RATE_LIMIT_BURST as usize) {
        assert!(limiter.check_and_consume(":1.AAA", now).is_ok());
    }
    assert!(limiter.check_and_consume(":1.AAA", now).is_err());

    // Sender B is completely unaffected and has full burst capacity
    for _ in 0..(RATE_LIMIT_BURST as usize) {
        assert!(limiter.check_and_consume(":1.BBB", now).is_ok());
    }
}

#[test]
fn test_sender_removal() {
    let mut limiter = RateLimiter::new();
    let now = Instant::now();
    let sender = ":1.REMOVE_ME";

    // Exhaust
    for _ in 0..(RATE_LIMIT_BURST as usize) {
        assert!(limiter.check_and_consume(sender, now).is_ok());
    }
    assert!(limiter.check_and_consume(sender, now).is_err());

    // Remove sender from limiter
    limiter.remove_sender(sender);

    // Re-adding gives fresh burst bucket
    assert!(limiter.check_and_consume(sender, now).is_ok());
}

#[test]
fn test_tracked_senders_pruning_and_cap() {
    let mut limiter = RateLimiter::new();
    let start = Instant::now();

    // Fill table with 256 senders at t=0
    for i in 0..256 {
        let sender = format!(":1.{i}");
        assert!(limiter.check_and_consume(&sender, start).is_ok());
    }

    // Advance 61 seconds so older senders are eligible for pruning
    let later = start + Duration::from_secs(61);
    // Adding 257th sender triggers prune of inactive buckets and succeeds
    assert!(limiter.check_and_consume(":1.NEW_SENDER", later).is_ok());

    // Test hitting the absolute global limit if all 256 are actively maintained at the same timestamp
    let mut full_limiter = RateLimiter::new();
    for i in 0..256 {
        let sender = format!(":1.ACTIVE_{i}");
        assert!(full_limiter.check_and_consume(&sender, start).is_ok());
    }
    // Attempting 257th without any elapsed time fails with LimitsExceeded
    let err = full_limiter.check_and_consume(":1.OVERFLOW", start);
    assert_eq!(
        err,
        Err(NotificationError::LimitsExceeded(
            "Global concurrent sender limit exceeded"
        ))
    );
}
