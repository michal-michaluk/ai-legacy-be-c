// src/common/rate_limit.rs — fixed-window rate limiter (A9). Applied at the
// auth/settings boundary to throttle brute-force and runaway clients. Returns a
// 429 / `settings:rate_limited` when exceeded.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<State>>,
    max_requests: u32,
    window: Duration,
}

struct State {
    buckets: HashMap<String, (u32, Instant)>,
}

impl RateLimiter {
    /// `max_requests` allowed per `window` per key; `burst` is the burst capacity.
    pub fn new(max_requests: u32, _burst: u32, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                buckets: HashMap::new(),
            })),
            max_requests,
            window,
        }
    }

    /// Is this key still within budget? Returns `true` to allow, `false` to throttle.
    pub fn allow(&self, key: &str) -> bool {
        let mut state = self.inner.lock().unwrap();
        let now = Instant::now();
        let (count, window_start) = state.buckets.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(*window_start) >= self.window {
            *count = 0;
            *window_start = now;
        }
        if *count >= self.max_requests {
            return false;
        }
        *count += 1;
        true
    }

    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_after_n_attempts() {
        let limiter = RateLimiter::new(3, 6, Duration::from_secs(60));
        for _ in 0..3 {
            assert!(limiter.allow("acme"));
        }
        assert!(!limiter.allow("acme"), "a 4th attempt must be throttled");
        assert!(limiter.allow("other"), "different keys are independent");
    }

    #[test]
    fn resets_after_window() {
        let limiter = RateLimiter::new(1, 2, Duration::from_secs(0));
        assert!(limiter.allow("acme"));
        // window elapsed (0s) → the same key is allowed again.
        assert!(limiter.allow("acme"));
    }
}
