use std::time::Instant;

pub struct TokenBucket {
    client_id: String,
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
    pub fn new(client_id: String, capacity: u64, rate_per_second: u64) -> Self {
        Self {
            client_id,
            capacity,
            remaining_tokens: capacity,
            refill_rate_per_second: rate_per_second,
            last_request_date: Instant::now(),
        }
    }

    pub fn matches_client_id(&self, client_id: &str) -> bool {
        self.client_id == client_id
    }

    pub fn is_allowed(&mut self) -> AllowedTokenRequest {
        let now = Instant::now();

        let elapsed_seconds = now.duration_since(self.last_request_date).as_secs();
        let refilling_tokens = elapsed_seconds * self.refill_rate_per_second;
        let mut tokens_available = refilling_tokens + self.remaining_tokens;
        if tokens_available > self.capacity { tokens_available = self.capacity; }

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
    #[test]
    fn should_return_true_when_match_client_ids() {
        let bucket = TokenBucket::new("client_id".to_string(), 1, 1);
        assert_eq!(bucket.matches_client_id("client_id"), true);
    }

    #[test]
    fn should_return_false_when_unmatch_client_ids() {
        let bucket = TokenBucket::new("client_id".to_string(), 1, 1);
        assert_eq!(bucket.matches_client_id("client_2"), false);
    }

    mod rate_limiting {
        use super::*;
        use std::{sync::Mutex, thread, time::Duration};
        #[test]
        fn should_allowed_when_bucket_has_tokens() {
            let mut bucket = TokenBucket::new("client_id".to_string(), 1, 1);
            let response: AllowedTokenRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);
        }

        #[test]
        fn should_deny_when_bucket_does_not_have_tokens() {
            let mut bucket = TokenBucket::new("client_id".to_string(), 1, 1);
            bucket.is_allowed();
            let response = bucket.is_allowed();

            assert_eq!(response.allowed, false);
            assert_eq!(response.remaining_tokens, 0);
        }

        #[test]
        fn should_refilled_bucket_when_2_seconds_passed() {
            let mut bucket = TokenBucket::new("client_id".to_string(), 5, 2);
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
        fn should_do_not_refill_the_bucket_after_max_token_capacity() {
            let mut bucket = TokenBucket::new("client_id".to_string(), 1, 1);
            let response: AllowedTokenRequest = bucket.is_allowed();

            assert_eq!(response.allowed, true);
            assert_eq!(response.remaining_tokens, 0);

            thread::sleep(Duration::from_secs(3));

            let response1: AllowedTokenRequest = bucket.is_allowed();
            assert_eq!(response1.allowed, true);
            assert_eq!(response1.remaining_tokens, 0);
        }

        #[test]
        fn should_allowed_3_requests_and_deny_1_request_with_concurrent_requests()
         {
            let bucket = Mutex::new(TokenBucket::new("client".to_string(), 3, 1));

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
