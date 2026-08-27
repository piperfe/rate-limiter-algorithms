# Testing Strategy

This document describes the layered testing approach and the conventions each layer follows. Individual test names are not listed here — the test files are the source of truth. This document explains *why* each layer exists and what belongs in it.

## Layers

| Layer | File | Owns | Must not duplicate |
|---|---|---|---|
| Value object | `src/window_unit.rs` | Unit-to-seconds conversion, elapsed-unit counting | Algorithm behaviour |
| Domain | `src/token_bucket.rs`, `src/fixed_window.rs` | Allow/deny decisions, replenishment timing, thread safety | Time arithmetic, HTTP concerns |
| Endpoint | `src/web_server.rs` | HTTP status codes, header format, configuration loading, per-client isolation | Algorithm internals |

Each file groups its tests into nested `mod` blocks named after business concepts rather than implementation details. Read the file to see the current groups.

## Value Object Tests (`src/window_unit.rs`)

**Scope:** Pure arithmetic — no clock, no sleeping.

`WindowUnit::elapsed_time_units()` is the seam that keeps time-dependent behaviour cheap to test. Confirming that a `Minutes` bucket only replenishes after 60 real seconds does not require a 60-second test: the conversion and its integer truncation are asserted directly, while the domain layer proves the wiring using injected `Instant` offsets.

This is also the only practical way to cover `Days` and `Hours` — sleeping a day was never an option, so those branches went unverified while the conversion was inlined in each algorithm.

Both algorithms share this conversion, so it is tested once rather than once per algorithm.

## Domain Unit Tests (`src/token_bucket.rs`, `src/fixed_window.rs`)

**Scope:** No HTTP, no async, no environment configuration.

Test names describe business behaviour — whether a request is allowed, whether tokens replenish, whether a limit holds under concurrency — not the methods being called.

The two algorithms differ in a way the groupings reflect: a token bucket *accrues* tokens continuously and must clamp at capacity, so capping is a real invariant there. A fixed window *resets* to capacity outright at the boundary, so there is no accumulation to clamp and no capping concept. Do not port a capping test from one to the other.

Timing tests take an injected `now: Instant` and advance it with `Duration` offsets — never `thread::sleep`. This makes exact-boundary assertions possible (e.g. denied at 999ms, allowed at 1000ms) and keeps the suite instant regardless of the configured unit. See [ADR-009](./decisions/ADR-009-clock-injection.md) for why. Longer units (`Minutes`, `Hours`, `Days`) are cheap to express this way and should be used wherever a test needs to distinguish real scaling from the `Seconds` special case where `in_seconds() == 1`.

## Endpoint Integration Tests (`src/web_server.rs`)

**Scope:** Async + HTTP + configuration + multi-client state.

Test names describe observable HTTP outcomes — status codes, header contents, per-client independence — not internal state.

This layer owns configuration loading, so tests that vary environment variables belong here rather than in the domain layer.

## Running Tests

```bash
# All tests
cargo test

# One layer (any module path works as a filter)
cargo test --lib window_unit
cargo test --lib token_bucket
cargo test --lib fixed_window
cargo test web_server::integration_tests

# One group within a layer
cargo test --lib token_bucket::tests::concurrency

# Show output during test
cargo test -- --nocapture

# Single-threaded (useful for debugging)
cargo test -- --test-threads=1
```

`cargo test` filters on a substring of the full module path, so any prefix — file, group, or test name — narrows the run.

## Test Conventions

### Environment Variable Management

Endpoint tests that need custom configuration use `temp_env::async_with_vars`, **and must be marked `#[serial]`**:

```rust
#[tokio::test]
#[serial]
async fn some_test() {
    let vars = [
        ("CAPACITY", Some("10")),
        ("UNIT_TIME", Some("Seconds")),
        ("REFILL_RATE_PER_UNIT_TIME", Some("1")),
    ];

    temp_env::async_with_vars(vars, (|| async {
        // Test code runs here with custom env vars
        let routes = create_routes();
        // ...
    })()).await;
}
```

See the `configuration` group in `src/web_server.rs` for the applied form.

**Why `temp_env`?**
- Scopes env var changes to a single test body
- Restores the previous state afterwards, so nothing leaks past the test

**Why `#[serial]` is also required.** `temp_env` restores state correctly but cannot isolate *during* the overlap. Env vars are process-global, Rust runs tests concurrently in one process, and `create_routes()` reads the environment at call time — so a test running alongside one that has `CAPACITY=10` in scope will observe `10` instead of the default. This produced a reproducible ~1-in-3 failure before `serial_test` was introduced.

The rule: **any test whose assertions depend on configuration needs `#[serial]`** — both the tests that set env vars and the tests that rely on defaults. Tests that are indifferent to config (the `bad_request` group, which asserts a 400 regardless) stay parallel.

This serializes those tests against each other only. Domain tests in `token_bucket`, `fixed_window`, and `window_unit` continue running in parallel, so total suite time is unaffected.

### Async Test Support

All endpoint tests use `#[tokio::test]` to enable async test execution:

```rust
#[tokio::test]
async fn some_test() {
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
cargo test <test_name> -- --nocapture
```

### Add Custom Logging

```rust
#[tokio::test]
async fn some_test() {
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
