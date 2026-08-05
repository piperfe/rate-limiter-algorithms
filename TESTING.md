# Testing Strategy

This document describes the two-tier testing approach: domain unit tests and endpoint integration tests.

## Test Structure

```
src/token_bucket.rs
  ├─ #[cfg(test)] mod tests
  │   ├─ Client Matching Tests
  │   └─ mod rate_limiting (integration with TokenBucket logic)
  │       ├─ Token Consumption
  │       ├─ Token Refill
  │       ├─ Capacity Overflow
  │       └─ Concurrent Access
  │
src/web_server.rs
  └─ #[cfg(test)] mod integration_tests
      ├─ New Client Initialization
      ├─ Sequential Requests
      ├─ Rate Limit Denials
      └─ Concurrent Multi-Client
```

## Domain Unit Tests (`src/token_bucket.rs`)

**Purpose:** Verify TokenBucket algorithm correctness in isolation.

**Scope:** No HTTP, no async, no environment configuration.

### Test Categories

#### Client Matching
- `should_return_true_when_match_client_ids` — Verify `matches_client_id()` correctly identifies clients
- `should_return_false_when_unmatch_client_ids` — Ensure different client IDs don't match

#### Token Consumption
- `should_allowed_when_bucket_has_tokens` — First request consumes one token
- `should_deny_when_bucket_does_not_have_tokens` — Exhausted bucket denies requests
- Verifies remaining tokens decrement correctly

#### Token Refill
- `should_refilled_bucket_when_2_seconds_passed` — Tokens accumulate after elapsed time
- `should_do_not_refill_the_bucket_after_max_token_capacity` — Refill caps at capacity

#### Concurrency
- `should_allowed_3_requests_and_deny_1_request_with_concurrent_requests` — Thread safety via Mutex
  - Spawns 4 threads competing for 3 tokens
  - Verifies exactly 1 is denied (serialized access)

## Endpoint Integration Tests (`src/web_server.rs`)

**Purpose:** Verify full HTTP request/response cycle and RFC compliance.

**Scope:** Async + HTTP + configuration + multi-client state.

### Test Categories

#### New Client Initialization

- `should_setting_the_bucket_for_new_users_using_env_vars` — Custom capacity via env vars
  - Sets `BUCKET_CAPACITY=10`, verifies bucket created with 10 tokens
  - First request consumes 1, leaving 9

- `should_setting_the_bucket_for_new_users_using_default_values` — Defaults when env unset
  - No env vars set, verifies capacity defaults to 60
  - First request leaves 59 remaining

#### Sequential Requests

- `should_decreasing_requests_in_the_bucket_for_old_users` — Tokens decrement per request
  - Makes 3 sequential requests for same client
  - Verifies remaining tokens: 59 → 58 → 57

#### Rate Limit Denials

- `should_deny_a_request` — 429 returned when bucket exhausted
  - Makes 61 requests (exceeds default capacity of 60)
  - Verifies 61st request returns 429
  - Checks RateLimit header shows 0 remaining

#### Concurrent Multi-Client

- `should_accepting_and_denying_concurrent_requests_for_one_user` — Concurrent load on single client
  - 119 concurrent requests for one client
  - Verifies exactly 60 allowed, 59 denied

- `should_accepting_and_denying_concurrent_requests_for_multiple_users` — Independent quotas
  - 119 iterations, each spawning 2 requests (client_1 + client_2)
  - Verifies client_1 has independent 60/59 split
  - Verifies client_2 has independent 60/59 split
  - Confirms no quota sharing between clients

## Running Tests

```bash
# All tests
cargo test

# Domain tests only
cargo test --lib token_bucket

# Endpoint tests only
cargo test web_server::integration_tests

# Specific test
cargo test should_deny_a_request -- --nocapture

# Show output during test
cargo test -- --nocapture

# Single-threaded (useful for debugging)
cargo test -- --test-threads=1
```

## Test Conventions

### Environment Variable Management

Endpoint tests that need custom configuration use `temp_env::async_with_vars`:

```rust
#[tokio::test]
async fn should_setting_the_bucket_for_new_users_using_env_vars() {
    let vars = [
        ("BUCKET_CAPACITY", Some("10")),
        ("BUCKET_REFILL_RATE_PER_SECOND", Some("1")),
    ];
    
    temp_env::async_with_vars(vars, (|| async {
        // Test code runs here with custom env vars
        let routes = create_routes();
        // ...
    })()).await;
}
```

**Why `temp_env`?**
- Isolates env var changes to specific test
- Automatically restores previous state (prevents test pollution)
- Safe for concurrent async tests

### Async Test Support

All endpoint tests use `#[tokio::test]` to enable async test execution:

```rust
#[tokio::test]
async fn should_deny_a_request() {
    // Can use .await inside
}
```

## Coverage Notes

### What's Tested

✓ Token bucket algorithm (consumption, refill, capacity)  
✓ Per-client independence  
✓ Concurrent access safety  
✓ HTTP status codes (200, 429)  
✓ RateLimit header format  
✓ Environment configuration  
✓ Default values  

### What's NOT Tested (Out of Scope)

- Network-level concerns (TCP errors, timeouts)
- OS-level load balancing (multiple server instances)
- Distributed rate limiting (Redis backend — future feature)
- Performance benchmarks (covered separately)

## Debugging Tests

### View Logs During Test

```bash
# Print debug info
RUST_LOG=debug cargo test -- --nocapture

# Single test with output
cargo test should_deny_a_request -- --nocapture
```

### Add Custom Logging

```rust
#[tokio::test]
async fn should_deny_a_request() {
    println!("Test starting...");
    
    let routes = create_routes();
    println!("Routes created");
    
    // ...
}
```

### Avoid Parallel Execution

```bash
# Run tests serially (slower but easier to debug)
cargo test -- --test-threads=1
```

## Future Testing Enhancements

- [ ] Benchmark suite (latency, throughput)
- [ ] Property-based testing (quickcheck for invariants)
- [ ] Load testing (concurrent client simulation)
- [ ] Distributed rate limiting tests (multi-instance)
