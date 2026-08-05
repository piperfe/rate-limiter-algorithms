# ADR-004: temp_env for Async Test Environment Variable Management

**Status:** Accepted  
**Date:** 2026-08-05  
**Author:** Rate Limiter Team

## Context

Integration tests require custom environment variables:
```rust
#[tokio::test]
async fn test_custom_capacity() {
    // Need BUCKET_CAPACITY=10 for this test
    // But other tests need BUCKET_CAPACITY=60
}
```

Challenges:
- `std::env::set_var()` is **unsafe** (modifies global state)
- Global state persists across tests → **test pollution**
- Concurrent test execution → **race conditions**

Need a safe, isolated mechanism for test-only env var changes.

## Decision

**Use temp_env crate with async_with_vars for test isolation.**

```rust
#[tokio::test]
async fn test_custom_capacity() {
    let vars = [("BUCKET_CAPACITY", Some("10"))];
    
    temp_env::async_with_vars(vars, (|| async {
        // BUCKET_CAPACITY=10 here
        let config = envy::from_env::<AppConfig>()?;
        assert_eq!(config.bucket_capacity, 10);
        // Automatically restored after this block
    })()).await;
}
```

### Rationale

#### 1. Safety Over Unsafety

```rust
// ❌ Unsafe (modifies global state)
std::env::set_var("BUCKET_CAPACITY", "10");
// ... test code ...
// State leaks to other tests!

// ✅ Safe (scoped isolation)
temp_env::async_with_vars([...], (|| async {
    // BUCKET_CAPACITY=10 only here
    // Automatically restored on exit
})()).await;
```

`std::env::set_var()` requires unsafe block but doesn't actually provide safety guarantees—the danger is test pollution, not memory unsafety.

#### 2. Automatic Restoration

Even on panic, temp_env restores env vars:

```rust
#[tokio::test]
async fn test_with_panic() {
    temp_env::async_with_vars([("VAR", Some("value"))], (|| async {
        // VAR is set
        panic!("Oops!");
        // ↓ Even on panic, VAR is restored ↓
    })()).await;
    
    // VAR is now restored for next test
}
```

#### 3. Prevents Test Pollution

Without isolation, test order becomes significant:

```bash
# Test A sets BUCKET_CAPACITY=10
# Test B expects default 60 but gets 10 ← FAILS (order-dependent)
# Test C runs before A ← now passes

cargo test  # Fails
cargo test --test-threads=1 B C A  # Passes
# ↑ Non-deterministic, hard to debug
```

With temp_env:

```bash
# Each test has its own scope
# Order irrelevant
cargo test  # Always passes
```

#### 4. Async Support

Unlike `std::env::set_var()`, temp_env works with async test blocks:

```rust
#[tokio::test]
async fn test_async() {
    temp_env::async_with_vars([...], (|| async {
        let config = envy::from_env::<AppConfig>()?;
        let response = server.get("/").await;  // ← async operations
        assert_eq!(response.status(), 200);
    })()).await;
}
```

### Trade-offs & Alternatives

#### 1. Manual unsafe + cleanup
```rust
#[tokio::test]
async fn test_config() {
    unsafe { std::env::set_var("BUCKET_CAPACITY", "10"); }
    
    // Test code
    
    unsafe { std::env::remove_var("BUCKET_CAPACITY"); }
}
```

**Pros:** No external dependency  
**Cons:**
- Cleanup can be skipped (panic = leak)
- Requires unsafe block (signals danger, but doesn't enforce it)
- Non-idiomatic Rust

**Verdict:** temp_env is the standard pattern; no reason to reinvent.

#### 2. Multiple Config Functions
```rust
fn create_routes_default() { ... }
fn create_routes_custom(capacity: u64) { ... }

#[tokio::test]
async fn test_custom_capacity() {
    let routes = create_routes_custom(10);
    // Test with custom capacity
}
```

**Pros:** No env var manipulation  
**Cons:**
- Doesn't test actual env config path (production uses envy)
- Duplicates setup logic
- Doesn't match real startup

**Verdict:** Insufficient; we need to test env var loading, not just behavior.

#### 3. Test Fixtures / Database Transactions
```rust
#[tokio::test]
#[serial]  // Run only one test at a time
async fn test_config() { ... }
```

**Pros:** Guaranteed isolation  
**Cons:**
- Defeats parallelism (slow CI/CD)
- Doesn't scale to many tests
- Hides concurrency bugs

**Verdict:** Works for 1-2 tests, not scalable.

#### 4. Docker Containers per Test
```bash
# Each test runs in isolated container with custom env
docker run -e BUCKET_CAPACITY=10 cargo test
```

**Pros:** Complete isolation  
**Cons:**
- Slow (container startup overhead)
- CI/CD infrastructure complexity
- Overkill for env var testing

**Verdict:** Over-engineered.

## Implementation

### Test Pattern

```rust
#[tokio::test]
async fn should_setting_the_bucket_for_new_users_using_env_vars() {
    let vars = [
        ("BUCKET_CAPACITY", Some("10")),
        ("BUCKET_REFILL_RATE_PER_SECOND", Some("1")),
    ];
    
    temp_env::async_with_vars(vars, (|| async {
        // All code here runs with custom env vars
        let routes = create_routes();
        let server = TestServer::new(routes);
        
        let response = server
            .get("/rate-limit")
            .add_header("X-API-Key", "client_1")
            .await;
        
        assert_eq!(response.status_code(), 200);
        assert_eq!(
            response.header("RateLimit").to_str().unwrap(),
            "\"api-v1\";r=9;t=1;pk=:client_1:"
        );
    })()).await;
}
```

### Dependency

Add to `Cargo.toml`:
```toml
[dev-dependencies]
temp-env = { version = "0.3", features = ["async_closure"] }
```

**Feature flag:** `async_closure` required for async test blocks (Rust 1.64+).

### Gotchas

**1. Must call closure with ():**
```rust
// ❌ Wrong (passes closure, not future)
temp_env::async_with_vars([...], || async { ... }).await;

// ✅ Correct (calls closure to get future)
temp_env::async_with_vars([...], (|| async { ... })()).await;
```

**2. Variable name typos are silent:**
```rust
temp_env::async_with_vars([("BUCKET_CAPACTY", Some("10"))], ...)
// ↑ Typo! But env var is set, so test doesn't catch it
// Use grep to validate var names
```

## Validation

✓ Tests use `async_with_vars` for isolation  
✓ No `unsafe { set_var() }` in test code  
✓ Tests pass in any order  
✓ Tests pass with `--test-threads=1` or parallel  
✓ Cleanup automatic even on panic

## Future Enhancements

- [ ] Add helper macro for common vars
- [ ] Log which vars are set in tests (debugging)
- [ ] Validate env var names against schema

## References

- [temp-env: Isolated environment variable testing](https://docs.rs/temp-env/latest/temp_env/)
- [std::env::set_var unsafe documentation](https://doc.rust-lang.org/std/env/fn.set_var.html)
- [Tokio: Running Tests](https://tokio.rs/tokio/topics/testing)
