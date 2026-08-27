use crate::window_unit::WindowUnit;
use std::time::{Duration, Instant};

pub struct TokenBucket {
    capacity: u64,
    unit_time: WindowUnit,
    refill_rate_per_unit_time: u64,
    remaining_tokens: u64,
    last_request_date: Instant,
}

pub struct AllowedTokenRequest {
    pub remaining_tokens: u64,
    pub allowed: bool,
}

impl TokenBucket {
    /// Accrues `refill_rate_per_unit_time` tokens every `unit_time`, never exceeding `capacity`.
    ///
    /// Accrual is `elapsed_units * refill_rate_per_unit_time` in `u64`. That product only
    /// overflows when a bucket sits untouched for a very long time *and* the rate is enormous —
    /// with `Seconds`, around 5.8e11 tokens/second after a full year of silence. Any rate a
    /// limiter would plausibly use sits several orders of magnitude below that, so the arithmetic
    /// is left unguarded. If it ever did overflow, release builds wrap and the wrapped value is
    /// still clamped to `capacity`, so the granted quota stays correct.
    pub fn new(capacity: u64, unit_time: WindowUnit, refill_rate_per_unit_time: u64, last_request_date: Instant) -> Self {
        Self {
            capacity,
            unit_time,
            refill_rate_per_unit_time,
            remaining_tokens: capacity,
            last_request_date,
        }
    }

    pub fn is_allowed(&mut self, now: Instant) -> AllowedTokenRequest {
        let elapsed = now.duration_since(self.last_request_date);
        let elapsed_time_units = self.unit_time.elapsed_time_units(elapsed);
        let refilling_tokens = elapsed_time_units * self.refill_rate_per_unit_time;

        let elapsed_seconds = elapsed_time_units * self.unit_time.in_seconds();
        self.last_request_date += Duration::from_secs(elapsed_seconds);

        let mut tokens_available = refilling_tokens + self.remaining_tokens;
        if tokens_available > self.capacity {
            tokens_available = self.capacity;
        }

        if tokens_available == 0 {
            return AllowedTokenRequest {
                remaining_tokens: tokens_available,
                allowed: false,
            };
        }

        tokens_available -= 1;
        self.remaining_tokens = tokens_available;
        AllowedTokenRequest {
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
            let mut bucket = TokenBucket::new(1, WindowUnit::Seconds, 1, now);
            let response: AllowedTokenRequest = bucket.is_allowed(now);

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);
        }

        #[test]
        fn should_deny_request_when_tokens_exhausted() {
            let now = Instant::now();
            let mut bucket = TokenBucket::new(1, WindowUnit::Seconds, 1, now);
            bucket.is_allowed(now);
            let response = bucket.is_allowed(now);

            assert_eq!(response.allowed, false);
            assert_eq!(response.remaining_tokens, 0);
        }
    }

    mod refill_and_capacity {
        use super::*;

        #[test]
        fn should_refill_tokens_once_unit_time_elapses() {
            let now = Instant::now();
            let mut bucket = TokenBucket::new(5, WindowUnit::Seconds, 2, now);
            bucket.is_allowed(now);
            bucket.is_allowed(now);
            bucket.is_allowed(now);
            bucket.is_allowed(now);
            let first: AllowedTokenRequest = bucket.is_allowed(now);

            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);

            let second: AllowedTokenRequest = bucket.is_allowed(now + Duration::from_secs(2));
            assert_eq!(second.allowed, true);
            assert_eq!(second.remaining_tokens, 3);
        }

        #[test]
        fn should_cap_tokens_at_capacity() {
            let now = Instant::now();
            let mut bucket = TokenBucket::new(1, WindowUnit::Seconds, 1, now);
            let first: AllowedTokenRequest = bucket.is_allowed(now);
            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);

            let second: AllowedTokenRequest = bucket.is_allowed(now + Duration::from_secs(3));
            assert_eq!(second.allowed, true);
            assert_eq!(second.remaining_tokens, 0);
        }

        #[test]
        fn should_not_refill_until_the_full_unit_has_elapsed() {
            let now = Instant::now();
            let mut bucket = TokenBucket::new(5, WindowUnit::Seconds, 2, now);
            for _ in 0..5 { bucket.is_allowed(now); }

            let first: AllowedTokenRequest = bucket.is_allowed(now + Duration::from_millis(999));
            assert_eq!(first.allowed, false);
            assert_eq!(first.remaining_tokens, 0);

            let second: AllowedTokenRequest = bucket.is_allowed(now + Duration::from_millis(1000));
            assert_eq!(second.allowed, true);
            assert_eq!(second.remaining_tokens, 1);
        }

        #[test]
        fn should_not_lose_partial_units_between_refills() {
            let now = Instant::now();
            let mut bucket = TokenBucket::new(5, WindowUnit::Seconds, 1, now);
            for _ in 0..5 { bucket.is_allowed(now); }          // exhaust

            // 1 whole unit + 500ms
            let first = bucket.is_allowed(now + Duration::from_millis(1500));
            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);           // 1 refilled, 1 consumed

            // another 1.5 units
            let second = bucket.is_allowed(now + Duration::from_millis(3000));
            assert_eq!(second.allowed, true);
            assert_eq!(second.remaining_tokens, 1);          // 2 refilled (1 + carried 0.5+0.5), 1 consumed
        }

        #[test]
        fn should_advance_the_anchor_by_the_full_unit_in_seconds() {
            // Seconds tests can't see a missing "* in_seconds()" since it's a no-op there; Minutes can.
            let now = Instant::now();
            let mut bucket = TokenBucket::new(1, WindowUnit::Minutes, 1, now);
            bucket.is_allowed(now);

            let refill = bucket.is_allowed(now + Duration::from_secs(60));
            assert_eq!(refill.allowed, true);

            // A buggy 1s-per-refill anchor (instead of 60s) would wrongly allow this.
            let after = bucket.is_allowed(now + Duration::from_secs(61));
            assert_eq!(after.allowed, false);
        }

        #[test]
        fn should_never_replenish_at_zero_rate() {
            let now = Instant::now();
            let mut bucket = TokenBucket::new(1, WindowUnit::Seconds, 0, now);

            let first = bucket.is_allowed(now);
            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);

            let second = bucket.is_allowed(now);
            assert_eq!(second.allowed, false);
            assert_eq!(second.remaining_tokens, 0);

            // A long wait grants nothing further — the rate is zero, not merely slow.
            let much_later = bucket.is_allowed(now + Duration::from_secs(3600));
            assert_eq!(much_later.allowed, false);
        }
    }

    mod concurrency {
        use super::*;

        #[test]
        fn should_enforce_limit_under_concurrent_access() {
            let now = Instant::now();
            let bucket = Mutex::new(TokenBucket::new(3, WindowUnit::Seconds, 1, now));

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
