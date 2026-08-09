use std::time::Instant;

pub struct TokenBucket {
    capacity: u64,
    remaining_tokens: u64,
    refill_rate_per_second: u64,
    last_request_date: Instant,
}

pub struct AllowedTokenRequest {
    pub remaining_tokens: u64,
    pub allowed: bool,
}

impl TokenBucket {
    pub fn new(capacity: u64, rate_per_second: u64) -> Self {
        Self {
            capacity,
            remaining_tokens: capacity,
            refill_rate_per_second: rate_per_second,
            last_request_date: Instant::now(),
        }
    }

    pub fn is_allowed(&mut self) -> AllowedTokenRequest {
        let now = Instant::now();

        let elapsed_seconds = now.duration_since(self.last_request_date).as_secs();
        let refilling_tokens = elapsed_seconds * self.refill_rate_per_second;
        let mut tokens_available = refilling_tokens + self.remaining_tokens;
        if tokens_available > self.capacity {
            tokens_available = self.capacity;
        }

        self.remaining_tokens = tokens_available;
        self.last_request_date = now;

        if tokens_available == 0 {
            self.last_request_date = now;
            return AllowedTokenRequest {
                remaining_tokens: self.remaining_tokens,
                allowed: false,
            };
        }

        self.remaining_tokens -= 1;
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
            let mut bucket = TokenBucket::new(1, 1);
            let response: AllowedTokenRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);
        }

        #[test]
        fn should_deny_request_when_tokens_exhausted() {
            let mut bucket = TokenBucket::new(1, 1);
            bucket.is_allowed();
            let response = bucket.is_allowed();

            assert_eq!(response.allowed, false);
            assert_eq!(response.remaining_tokens, 0);
        }
    }

    mod refill_and_capacity {
        use super::*;

        #[test]
        fn should_refill_tokens_after_elapsed_time() {
            let mut bucket = TokenBucket::new(5, 2);
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
            let mut bucket = TokenBucket::new(1, 1);
            let response: AllowedTokenRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);

            thread::sleep(Duration::from_secs(3));

            let response1: AllowedTokenRequest = bucket.is_allowed();
            assert_eq!(response1.allowed, true);
            assert_eq!(response1.remaining_tokens, 0);
        }
    }

    mod concurrency {
        use super::*;

        #[test]
        fn should_enforce_limit_under_concurrent_access() {
            let bucket = Mutex::new(TokenBucket::new(3, 1));

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
