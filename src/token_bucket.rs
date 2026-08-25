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

//TODO clock injection -> take `now: Instant` in `new` and `is_allowed` instead of calling
//     `Instant::now()` internally, so timing tests use exact offsets from a fixed origin
//     rather than `thread::sleep`. Today every timing test tolerates only ~500ms of sleep
//     overshoot before `elapsed_units` rounds to the next integer. Applies to FixedWindow too.
impl TokenBucket {
    /// Accrues `refill_rate_per_unit_time` tokens every `unit_time`, never exceeding `capacity`.
    ///
    /// Accrual is `elapsed_units * refill_rate_per_unit_time` in `u64`. That product only
    /// overflows when a bucket sits untouched for a very long time *and* the rate is enormous —
    /// with `Seconds`, around 5.8e11 tokens/second after a full year of silence. Any rate a
    /// limiter would plausibly use sits several orders of magnitude below that, so the arithmetic
    /// is left unguarded. If it ever did overflow, release builds wrap and the wrapped value is
    /// still clamped to `capacity`, so the granted quota stays correct.
    pub fn new(capacity: u64, unit_time: WindowUnit, refill_rate_per_unit_time: u64) -> Self {
        Self {
            capacity,
            unit_time,
            refill_rate_per_unit_time,
            remaining_tokens: capacity,
            last_request_date: Instant::now(),
        }
    }

    pub fn is_allowed(&mut self) -> AllowedTokenRequest {
        let now = Instant::now();

        let elapsed_seconds = now.duration_since(self.last_request_date).as_secs();
        let elapse_units = self.unit_time.elapsed_units(elapsed_seconds);
        let refilling_tokens = elapse_units * self.refill_rate_per_unit_time;

        let refilled_unit_time = elapse_units * self.unit_time.in_seconds();
        self.last_request_date += Duration::from_secs(refilled_unit_time);

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
    use std::{sync::Mutex, thread, time::Duration};

    mod allow_deny {
        use super::*;

        #[test]
        fn should_allow_request_when_tokens_available() {
            let mut bucket = TokenBucket::new(1, WindowUnit::Seconds, 1);
            let response: AllowedTokenRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);
        }

        #[test]
        fn should_deny_request_when_tokens_exhausted() {
            let mut bucket = TokenBucket::new(1, WindowUnit::Seconds, 1);
            bucket.is_allowed();
            let response = bucket.is_allowed();

            assert_eq!(response.allowed, false);
            assert_eq!(response.remaining_tokens, 0);
        }
    }

    mod refill_and_capacity {
        use super::*;

        #[test]
        fn should_refill_tokens_once_unit_time_elapses() {
            let mut bucket = TokenBucket::new(5, WindowUnit::Seconds, 2);
            bucket.is_allowed();
            bucket.is_allowed();
            bucket.is_allowed();
            bucket.is_allowed();
            let response: AllowedTokenRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);

            thread::sleep(Duration::from_secs(2));

            let response1: AllowedTokenRequest = bucket.is_allowed();
            assert_eq!(response1.allowed, true);
            assert_eq!(response1.remaining_tokens, 3);
        }

        #[test]
        fn should_cap_tokens_at_capacity() {
            let mut bucket = TokenBucket::new(1, WindowUnit::Seconds, 1);
            let response: AllowedTokenRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);

            thread::sleep(Duration::from_secs(3));

            let response1: AllowedTokenRequest = bucket.is_allowed();
            assert_eq!(response1.allowed, true);
            assert_eq!(response1.remaining_tokens, 0);
        }

        #[test]
        fn should_not_refill_until_the_full_unit_has_elapsed() {
            let mut bucket = TokenBucket::new(5, WindowUnit::Seconds, 2);
            for _ in 0..5 { bucket.is_allowed(); }

            thread::sleep(Duration::from_millis(500));

            let response: AllowedTokenRequest = bucket.is_allowed();
            assert_eq!(response.allowed, false);
            assert_eq!(response.remaining_tokens, 0);

            thread::sleep(Duration::from_millis(500));

            let response1: AllowedTokenRequest = bucket.is_allowed();
            assert_eq!(response1.allowed, true);
            assert_eq!(response1.remaining_tokens, 1);
        }

        #[test]
        fn should_not_lose_partial_units_between_refills() {
            let mut bucket = TokenBucket::new(5, WindowUnit::Seconds, 1);
            for _ in 0..5 { bucket.is_allowed(); }          // exhaust

            thread::sleep(Duration::from_millis(1500));      // 1 whole unit + 500ms

            let first = bucket.is_allowed();
            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);           // 1 refilled, 1 consumed

            thread::sleep(Duration::from_millis(1500));      // another 1.5 units

            let second = bucket.is_allowed();
            assert_eq!(second.allowed, true);
            assert_eq!(second.remaining_tokens, 1);          // 2 refilled (1 + carried 0.5+0.5), 1 consumed
        }
    }

    mod concurrency {
        use super::*;

        #[test]
        fn should_enforce_limit_under_concurrent_access() {
            let bucket = Mutex::new(TokenBucket::new(3, WindowUnit::Seconds, 1));

            let requests: Vec<_> = std::thread::scope(|scope| {
                let handles: Vec<_> = (0..4)
                    .map(|_| {
                        scope.spawn(|| {
                            let mut data = bucket.lock().unwrap();
                            data.is_allowed()
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
