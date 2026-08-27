use crate::window_unit::WindowUnit;
use std::time::{Duration, Instant};

pub struct FixedWindow {
    capacity: u64,
    unit_time: WindowUnit,
    remaining_tokens: u64,
    last_request_date: Instant,
}

pub struct AllowedFixedWindowRequest {
    pub remaining_tokens: u64,
    pub allowed: bool,
}

impl FixedWindow {
    pub fn new(capacity: u64, unit_time: WindowUnit, last_request_date: Instant) -> Self {
        Self {
            capacity,
            unit_time,
            remaining_tokens: capacity,
            last_request_date,
        }
    }

    pub fn is_allowed(&mut self, now: Instant) -> AllowedFixedWindowRequest {
        let elapsed = now.duration_since(self.last_request_date);
        let elapsed_time_units = self.unit_time.elapsed_time_units(elapsed);

        let mut tokens_available = self.remaining_tokens;
        let time_to_refill = elapsed_time_units > 0;
        if time_to_refill {
            tokens_available = self.capacity;

            let elapsed_seconds = elapsed_time_units * self.unit_time.in_seconds();
            self.last_request_date += Duration::from_secs(elapsed_seconds);
        }

        if tokens_available == 0 {
            return AllowedFixedWindowRequest {
                remaining_tokens: tokens_available,
                allowed: false,
            };
        }

        tokens_available -= 1;
        self.remaining_tokens = tokens_available;
        AllowedFixedWindowRequest {
            remaining_tokens: self.remaining_tokens,
            allowed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Mutex, time::Duration};

    mod allow_deny {
        use super::*;

        #[test]
        fn should_allow_request_when_tokens_available() {
            let now = Instant::now();
            let mut bucket = FixedWindow::new(10, WindowUnit::Minutes, now);
            let response: AllowedFixedWindowRequest = bucket.is_allowed(now);

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 9);
        }

        #[test]
        fn should_deny_request_when_tokens_exhausted() {
            let now = Instant::now();
            let mut bucket = FixedWindow::new(1, WindowUnit::Minutes, now);
            bucket.is_allowed(now);
            let response = bucket.is_allowed(now);

            assert_eq!(response.allowed, false);
            assert_eq!(response.remaining_tokens, 0);
        }
    }

    mod window_reset {
        use super::*;

        #[test]
        fn should_reset_tokens_to_capacity_once_window_elapses() {
            let now = Instant::now();
            let mut bucket = FixedWindow::new(2, WindowUnit::Seconds, now);
            bucket.is_allowed(now);
            let response: AllowedFixedWindowRequest = bucket.is_allowed(now);

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);

            let response1: AllowedFixedWindowRequest = bucket.is_allowed(now + Duration::from_secs(1));
            assert_eq!(response1.allowed, true);
            assert_eq!(response1.remaining_tokens, 1);
        }

        #[test]
        fn should_not_reset_until_the_full_window_has_elapsed() {
            let now = Instant::now();
            let mut bucket = FixedWindow::new(2, WindowUnit::Seconds, now);
            bucket.is_allowed(now);
            let first: AllowedFixedWindowRequest = bucket.is_allowed(now);
            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);

            // 1ms short of the window: still the old window, no reset
            let second: AllowedFixedWindowRequest = bucket.is_allowed(now + Duration::from_millis(999));
            assert_eq!(second.allowed, false);
            assert_eq!(second.remaining_tokens, 0);

            // exactly on the boundary: the window is inclusive, so this resets
            let third: AllowedFixedWindowRequest = bucket.is_allowed(now + Duration::from_millis(1000));
            assert_eq!(third.allowed, true);
            assert_eq!(third.remaining_tokens, 1);
        }

        #[test]
        fn should_deny_when_window_has_not_elapsed() {
            let now = Instant::now();
            let mut bucket = FixedWindow::new(2, WindowUnit::Minutes, now);
            bucket.is_allowed(now);
            let first: AllowedFixedWindowRequest = bucket.is_allowed(now);
            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);

            let second: AllowedFixedWindowRequest = bucket.is_allowed(now + Duration::from_secs(1));
            assert_eq!(second.allowed, false);
            assert_eq!(second.remaining_tokens, 0);
        }

        #[test]
        fn should_not_lose_partial_units_between_window_elapses() {
            let now = Instant::now();
            let mut bucket = FixedWindow::new(2, WindowUnit::Seconds, now);
            bucket.is_allowed(now);
            bucket.is_allowed(now);
            // 1 whole window + 500ms: resets to 2, anchor advances to 1s — not to 1.5s
            bucket.is_allowed(now + Duration::from_millis(1500));

            let first = bucket.is_allowed(now + Duration::from_millis(1500));
            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);

            // 1100ms past the anchor at 1s, so the carried 500ms is what crosses the boundary
            let second = bucket.is_allowed(now + Duration::from_millis(2100));
            assert_eq!(second.allowed, true);
            assert_eq!(second.remaining_tokens, 1);
        }

        #[test]
        fn should_advance_the_anchor_by_the_full_unit_in_seconds() {
            // Seconds tests can't see a missing "* in_seconds()" since it's a no-op there; Minutes can.
            let now = Instant::now();
            let mut bucket = FixedWindow::new(1, WindowUnit::Minutes, now);
            bucket.is_allowed(now);

            let reset = bucket.is_allowed(now + Duration::from_secs(60));
            assert_eq!(reset.allowed, true);

            // A buggy 1s-per-reset anchor (instead of 60s) would wrongly reset again here.
            let after = bucket.is_allowed(now + Duration::from_secs(61));
            assert_eq!(after.allowed, false);
        }
    }

    mod concurrency {
        use super::*;

        #[test]
        fn should_enforce_limit_under_concurrent_access() {
            let now = Instant::now();
            let bucket = Mutex::new(FixedWindow::new(3, WindowUnit::Minutes, now));

            let requests: Vec<_> = std::thread::scope(|scope| {
                let handles: Vec<_> = (0..4)
                    .map(|_| {
                        scope.spawn(|| {
                            let mut data = bucket.lock().unwrap();
                            data.is_allowed(now)
                        })
                    })
                    .collect();

                handles.into_iter().map(|handle| handle.join()).collect()
            });
            assert_eq!(
                requests
                    .iter()
                    .filter(|request| !request.as_ref().unwrap().allowed)
                    .count(),
                1
            );
            assert_eq!(
                requests
                    .iter()
                    .filter(|request| request.as_ref().unwrap().allowed)
                    .count(),
                3
            );
            assert_eq!(bucket.lock().unwrap().remaining_tokens, 0);
        }
    }
}
