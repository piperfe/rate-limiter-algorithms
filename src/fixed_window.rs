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

//TODO clock injection -> see the matching note on TokenBucket; same change applies here
impl FixedWindow {
    pub fn new(capacity: u64, unit_time: WindowUnit) -> Self {
        Self {
            capacity,
            unit_time,
            remaining_tokens: capacity,
            last_request_date: Instant::now(),
        }
    }

    pub fn is_allowed(&mut self) -> AllowedFixedWindowRequest {
        let now = Instant::now();
        let mut tokens_available = self.remaining_tokens;

        let elapsed_seconds = now.duration_since(self.last_request_date).as_secs();
        let elapsed_units = self.unit_time.elapsed_units(elapsed_seconds);

        let time_to_refill = elapsed_units > 0;
        if time_to_refill {
            tokens_available = self.capacity;

            let elapsed_unit_time = elapsed_units * self.unit_time.in_seconds();
            self.last_request_date += Duration::from_secs(elapsed_unit_time);
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
    use std::{sync::Mutex, thread, time::Duration};

    mod allow_deny {
        use super::*;

        #[test]
        fn should_allow_request_when_tokens_available() {
            let mut bucket = FixedWindow::new(10, WindowUnit::Minutes);
            let response: AllowedFixedWindowRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 9);
        }

        #[test]
        fn should_deny_request_when_tokens_exhausted() {
            let mut bucket = FixedWindow::new(1, WindowUnit::Minutes);
            bucket.is_allowed();
            let response = bucket.is_allowed();

            assert_eq!(response.allowed, false);
            assert_eq!(response.remaining_tokens, 0);
        }
    }

    mod window_reset {
        use super::*;

        #[test]
        fn should_reset_tokens_to_capacity_once_window_elapses() {
            let mut bucket = FixedWindow::new(2, WindowUnit::Seconds);
            bucket.is_allowed();
            let response: AllowedFixedWindowRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);

            thread::sleep(Duration::from_secs(1));

            let response1: AllowedFixedWindowRequest = bucket.is_allowed();
            assert_eq!(response1.allowed, true);
            assert_eq!(response1.remaining_tokens, 1);
        }

        #[test]
        fn should_not_reset_until_the_full_window_has_elapsed() {
            let mut bucket = FixedWindow::new(2, WindowUnit::Seconds);
            bucket.is_allowed();
            let response: AllowedFixedWindowRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);

            thread::sleep(Duration::from_millis(500));

            let response1: AllowedFixedWindowRequest = bucket.is_allowed();
            assert_eq!(response1.allowed, false);
            assert_eq!(response1.remaining_tokens, 0);

            thread::sleep(Duration::from_millis(500));

            let response2: AllowedFixedWindowRequest = bucket.is_allowed();
            assert_eq!(response2.allowed, true);
            assert_eq!(response2.remaining_tokens, 1);
        }

        #[test]
        fn should_deny_when_window_has_not_elapsed() {
            let mut bucket = FixedWindow::new(2, WindowUnit::Minutes);
            bucket.is_allowed();
            let response: AllowedFixedWindowRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);

            thread::sleep(Duration::from_secs(1));

            let response1: AllowedFixedWindowRequest = bucket.is_allowed();
            assert_eq!(response1.allowed, false);
            assert_eq!(response1.remaining_tokens, 0);
        }

        #[test]
        fn should_not_lose_partial_units_between_window_elapses() {
            let mut bucket = FixedWindow::new(2, WindowUnit::Seconds);
            bucket.is_allowed();
            bucket.is_allowed();

            thread::sleep(Duration::from_millis(1500));      // 1 whole window + 500ms

            bucket.is_allowed();                             // resets to 2, anchor advances to 1s (not 1.5s)
            let first = bucket.is_allowed();
            assert_eq!(first.allowed, true);
            assert_eq!(first.remaining_tokens, 0);           // both of the reset window's tokens now spent

            thread::sleep(Duration::from_millis(600));       // 500ms carried + 600ms = past the 2s boundary

            let second = bucket.is_allowed();
            assert_eq!(second.allowed, true);
            assert_eq!(second.remaining_tokens, 1);          // reset to 2, 1 consumed — only reachable if the 500ms carried
        }

    }

    mod concurrency {
        use super::*;

        #[test]
        fn should_enforce_limit_under_concurrent_access() {
            let bucket = Mutex::new(FixedWindow::new(3, WindowUnit::Minutes));

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
