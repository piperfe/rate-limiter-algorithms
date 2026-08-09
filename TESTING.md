# Testing Strategy

This document describes the two-tier testing approach: domain unit tests and endpoint integration tests.

## Test Structure

Tests are organized into nested modules by business concept (see `src/token_bucket.rs:54-143` and `src/web_server.rs:95-326`):

**Domain Tests** (`src/token_bucket.rs`):
- `mod allow_deny` — Token consumption and denial scenarios
- `mod refill_and_capacity` — Refill logic and capacity capping
- `mod concurrency` — Thread-safe concurrent access

**Endpoint Tests** (`src/web_server.rs`):
- `mod configuration` — Environment variable and default configuration
- `mod rate_limiting` — Token decrement and rate limit enforcement
- `mod concurrency` — Concurrent load (single and multi-client)

## Domain Unit Tests (`src/token_bucket.rs`)

**Purpose:** Verify TokenBucket algorithm correctness in isolation.

**Scope:** No HTTP, no async, no environment configuration.

### Test Categories

See `src/token_bucket.rs:54-143` for implementation.

#### Allow / Deny (`mod allow_deny`)
- `should_allow_request_when_tokens_available` — First request consumes one token
- `should_deny_request_when_tokens_exhausted` — Exhausted bucket denies requests

#### Refill & Capacity (`mod refill_and_capacity`)
- `should_refill_tokens_after_elapsed_time` — Tokens accumulate after elapsed time
- `should_cap_tokens_at_capacity` — Refill caps at capacity (no unbounded accumulation)

#### Concurrency (`mod concurrency`)
- `should_enforce_limit_under_concurrent_access` — Thread safety via Arc<DashMap>
  - Spawns 4 threads competing for 3 tokens
  - Verifies exactly 1 is denied (atomic access enforced)

## Endpoint Integration Tests (`src/web_server.rs`)

**Purpose:** Verify full HTTP request/response cycle and RFC compliance.

**Scope:** Async + HTTP + configuration + multi-client state.

### Test Categories

See `src/web_server.rs:95-326` for implementation.

#### Configuration (`mod configuration`)

- `should_return_200_with_custom_capacity_from_env_vars` — Custom capacity via env vars (see line 105)
- `should_return_200_with_default_capacity_for_new_client` — Defaults when env unset (see line 135)

#### Rate Limiting (`mod rate_limiting`)

- `should_return_200_with_decremented_tokens_on_repeat_request` — Sequential requests decrement tokens (see line 161)
- `should_return_429_when_tokens_exhausted` — 429 returned when capacity exceeded (see line 191)

#### Concurrency (`mod concurrency`)

- `should_enforce_limit_correctly_under_concurrent_load_single_client` — Concurrent load on single client (see line 227)
- `should_isolate_limits_between_concurrent_clients` — Independent per-client quotas (see line 255)

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
