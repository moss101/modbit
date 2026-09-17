//! Per-principal rate limiting (docs/33 "Cloud API implementation": rate
//! limits in the middleware): a token bucket per principal, refilled at a
//! steady rate; a request over the budget is refused `RATE_LIMITED` before
//! any handler runs.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// The limiter.
pub struct RateLimiter {
    capacity: f64,
    per_second: f64,
    buckets: Mutex<HashMap<String, (f64, Instant)>>,
}

impl RateLimiter {
    /// `capacity` requests at once, refilled at `per_second`.
    #[must_use]
    pub fn new(capacity: u32, per_second: f64) -> Self {
        Self {
            capacity: f64::from(capacity),
            per_second,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Take one token for `key`; `false` when the bucket is empty.
    pub fn admit(&self, key: &str) -> bool {
        self.admit_at(key, Instant::now())
    }

    /// `admit` as of `now` (the clock is the caller's, so the arithmetic
    /// is testable without sleeping).
    pub fn admit_at(&self, key: &str, now: Instant) -> bool {
        let mut b = self.buckets.lock().expect("buckets");
        let entry = b.entry(key.to_owned()).or_insert((self.capacity, now));
        let elapsed = now.duration_since(entry.1).as_secs_f64();
        entry.0 = (entry.0 + elapsed * self.per_second).min(self.capacity);
        entry.1 = now;
        if entry.0 >= 1.0 {
            entry.0 -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bucket_admits_its_capacity_then_refuses_until_refilled() {
        let r = RateLimiter::new(3, 1000.0);
        let t0 = Instant::now();
        assert!(r.admit_at("a", t0) && r.admit_at("a", t0) && r.admit_at("a", t0));
        assert!(!r.admit_at("a", t0));
        assert!(r.admit_at("b", t0), "another principal has its own bucket");
        let later = t0 + std::time::Duration::from_millis(5);
        assert!(r.admit_at("a", later), "refilled");
        assert!(r.admit_at("a", later) && r.admit_at("a", later));
        assert!(
            !r.admit_at("a", later),
            "the refill is capped at the capacity"
        );
    }
}
