# ADR-006: Separated Domain Unit Tests and Endpoint Integration Tests

**Status:** Accepted  
**Date:** 2026-08-05  
**Author:** Rate Limiter Team

## Context

Rate limiter requires testing at two levels:

**Level 1: Domain Logic**
- TokenBucket algorithm correctness
- Token consumption and refill
- Concurrency safety
- No HTTP involved

**Level 2: HTTP Contract**
- Request parsing
- Rate-limit headers
- 200 vs 429 status codes
- Multi-client independence
- Full HTTP request/response cycle

Architectural question: Single monolithic test suite vs. split by level?

## Decision

**Split into two test layers:**
1. **Domain unit tests** (`src/token_bucket.rs::tests`) — Algorithm correctness
2. **Endpoint integration tests** (`src/web_server.rs::integration_tests`) — HTTP contract

> **Extended 2026-08-25:** a third layer was added below the domain — **value object tests**
> (`src/window_unit.rs::tests`) covering period-to-seconds conversion and whole-unit counting.
> See *Extension* at the end of this ADR.

### Rationale

#### 1. Separation of Concerns

| Layer | Owns | Does NOT Test |
|---|---|---|
| Domain | Token bucket logic | HTTP details |
| Endpoint | HTTP request/response | Algorithm correctness (delegates to domain) |

Domain tests verify *what* the algorithm does:
```rust
#[test]
fn should_deny_when_bucket_does_not_have_tokens() {
    let mut bucket = TokenBucket::new("client", 1, 1);
    bucket.is_allowed();
    let result = bucket.is_allowed();
    assert_eq!(result.allowed, false);  // ← Verify algorithm
}
```

Endpoint tests verify *how* the algorithm is used in HTTP:
```rust
#[tokio::test]
async fn should_deny_a_request() {
    // Make 61 requests
    for i in 1..=61 {
        let response = server.get("/").add_header("X-API-Key", "client_1").await;
        if i <= 60 {
            assert_eq!(response.status_code(), 200);
        } else {
            assert_eq!(response.status_code(), 429);  // ← Verify HTTP
        }
    }
}
```

#### 2. Test Isolation & Speed

Domain tests are fast (no HTTP, no async):
```bash
$ cargo test --lib token_bucket
   running 7 tests
   
test result: ok. 7 passed; 0 failed; 0 ignored
   Finished in 3.01s
```

Endpoint tests are slower (async, tokio runtime, HTTP setup):
```bash
$ cargo test web_server::integration_tests
   running 6 tests
   
test result: ok. 6 passed; 0 failed; 0 ignored
   Finished in 0.01s  # (but startup overhead)
```

**Benefit:** Developers can run `cargo test --lib` for quick feedback without spawning async runtime.

#### 3. Test Debugging

Domain test failure:
```
thread 'token_bucket::tests::should_deny_when_bucket_does_not_have_tokens' panicked
⚠ Algorithm bug, no HTTP involved
→ Fix TokenBucket::is_allowed()
```

Endpoint test failure:
```
thread 'web_server::integration_tests::should_deny_a_request' panicked
   left: 200
   right: 429
⚠ Could be: algorithm bug, header parsing, status code mapping
→ Check: is_allowed() logic, HTTP handler, status code conversion
```

Separated tests pinpoint failure location.

#### 4. Avoiding Cross-Layer Duplication

**Without split (monolithic endpoint tests):**
```rust
// Endpoint test tries to verify algorithm AND HTTP
#[tokio::test]
async fn should_deny_a_request() {
    let mut bucket = TokenBucket::new("client", 1, 1);  // ← Algorithm test
    bucket.is_allowed();
    assert_eq!(bucket.is_allowed().allowed, false);     // ← Algorithm test
    
    // THEN endpoint test
    let response = server.get("/").await;
    assert_eq!(response.status_code(), 429);            // ← HTTP test
}
```

Problem: Algorithm logic tested twice (domain + endpoint), duplicate assertions.

**With split (current approach):**
- Domain tests: Algorithm only
- Endpoint tests: HTTP only (trusts domain layer)
- No duplication

**Removed duplicate:** `should_client_1_new_request_allowed_and_client_2_new_request_deny` (tested multi-client algorithm, not HTTP).

#### 5. Scalability

As more algorithms added (fixed window, sliding window):

```
Domain tests grow:
  src/token_bucket.rs::tests/
  src/fixed_window.rs::tests/
  src/sliding_window.rs::tests/
  
Endpoint tests stay flat:
  src/web_server.rs::integration_tests/
  (HTTP contract same regardless of algorithm)
```

Endpoint tests delegate to algorithm choice, don't duplicate per algorithm.

### Trade-offs & Alternatives

#### 1. Monolithic HTTP Tests
```rust
// Single test file, all tests are async + HTTP
#[tokio::test]
async fn should_work_with_token_bucket() { ... }
#[tokio::test]
async fn should_work_with_fixed_window() { ... }
```

**Pros:** Simple, everything in one place  
**Cons:**
- All tests pay async/HTTP overhead
- Slow feedback loop (even algorithm bug requires full HTTP stack)
- Tight coupling between algorithm and HTTP handler
- Duplication (algorithm verified in HTTP context, not in domain)

**Verdict:** Works for small projects, scales poorly.

#### 2. Separate Crates
```
rate-limiter/
  ├─ token_bucket/ (lib crate, only domain tests)
  └─ web-server/ (bin crate, only HTTP tests)
```

**Pros:** Clean separation  
**Cons:**
- Overkill for single project
- Cargo workspace overhead
- Inter-crate dependency management

**Verdict:** Unnecessary complexity for early-stage project.

#### 3. Property-Based Testing
```rust
use proptest::proptest;

proptest! {
    #[test]
    fn bucket_never_exceeds_capacity(
        elapsed in 0u64..100,
        rate in 1u64..10,
    ) {
        // Generate test cases
        // Verify property
    }
}
```

**Pros:** Finds edge cases automatically  
**Cons:**
- Slower (tests many cases)
- Harder to debug failures
- Orthogonal to unit/integration split

**Verdict:** Good *addition* to existing tests, not replacement.

#### 4. Snapshot Testing
```rust
#[tokio::test]
async fn should_match_http_snapshot() {
    let response = server.get("/").await;
    insta::assert_snapshot!(response);
}
```

**Pros:** Easy regression detection  
**Cons:**
- Brittle (snapshot must be updated manually)
- Hides *what* changed, not *why*
- Requires review of snapshot diffs

**Verdict:** Useful for HTTP response formats, complement to assertions.

## Implementation

### Domain Tests (`src/token_bucket.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    mod client_matching { ... }
    mod rate_limiting {
        mod token_consumption { ... }
        mod token_refill { ... }
        mod capacity_overflow { ... }
        mod concurrent_access { ... }
    }
}
```

Focus: Algorithm behavior, not HTTP.

### Endpoint Tests (`src/web_server.rs`)

```rust
#[cfg(test)]
mod integration_tests {
    use axum_test::TestServer;
    
    mod new_client_initialization { ... }
    mod sequential_requests { ... }
    mod rate_limit_denials { ... }
    mod concurrent_multi_client { ... }
}
```

Focus: HTTP request/response contract.

### Test Naming

**Domain:** Behavior-focused ("should_deny_when_...", "should_refill_after_...")  
**Endpoint:** Observable outcome ("returns 429 with...", "accepts 60 requests and denies...")

## Validation

✓ Domain tests: 7 passed  
✓ Endpoint tests: 6 passed  
✓ No cross-layer duplication (removed multi-client algo test)  
✓ Clear responsibility boundaries

## Future Enhancements

- [ ] Add domain tests for new algorithms (fixed window, sliding window)
- [ ] Add benchmark suite (separate perf/ crate)
- [ ] Add property-based tests (proptest)
- [ ] Add mutation testing (verify test quality)

## References

- [Rust: Testing guide](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Unit vs Integration Tests](https://doc.rust-lang.org/book/ch11-03-test-organization.html)
- [Testing Pyramid concept](https://martinfowler.com/bliki/TestPyramid.html)

## Extension (2026-08-25): a value-object layer beneath the domain

### Why a third layer

Time-unit conversion originally lived inline in each algorithm, which forced every timing behaviour to be verified by sleeping. Two consequences:

- **Cost.** Confirming a `Minutes` period required a 60-second test. Two such tests put the suite at **60.01s**.
- **Coverage.** `Days` and `Hours` were untestable in principle — nobody sleeps a day — so those branches shipped unverified.

Extracting `WindowUnit` into `src/window_unit.rs` made the conversion a pure function of its inputs. Asserting it directly takes microseconds and reaches every variant.

Suite went **60.01s → 3.01s** with strictly more coverage.

### Layer boundaries

| Layer | Owns | Does NOT test |
|---|---|---|
| Value object | Period arithmetic, whole-unit counting, remainder truncation | Allow/deny decisions |
| Domain | Allow/deny, replenishment timing, anchoring, thread safety | Time arithmetic, HTTP |
| Endpoint | Status codes, headers, config loading, client isolation | Algorithm internals |

Both algorithms share the conversion, so it is tested once rather than once per algorithm — the saving compounds as algorithms are added.

### The rule this creates

**Timing tests belong at the lowest layer that can express them.** A domain test may sleep one or two seconds using `WindowUnit::Seconds` to prove the wiring against a real clock; anything about how long a period *lasts* belongs to the value object.

This also covers boundary conditions no timing test can reach reliably. `FixedWindow`'s reset predicate is `elapsed_units > 0`, equivalent to the older `elapsed_seconds >= in_seconds()` since `a / b > 0` exactly when `a >= b` for `b > 0` — so "does exactly 60 seconds count?" is answered by an arithmetic assertion rather than a sleep that might overshoot.

### Known limitation (resolved 2026-08-26): clock injection

Domain timing tests originally called `Instant::now()` internally and tolerated only ~500ms of sleep overshoot before `elapsed_time_units` rounded to the next integer — stable locally, fragile on a loaded CI runner.

Resolved by injecting the clock: `TokenBucket::new`, `FixedWindow::new`, and both `is_allowed` methods now take `now: Instant` as a parameter instead of calling `Instant::now()` internally. Production code (`web_server.rs`) supplies the real clock once per request; tests supply exact offsets from a fixed origin.

This removed every remaining sleep from the domain layer. `token_bucket` and `fixed_window` dropped from 3.01s and 1.01s respectively to 0.00s, and tests can now assert the exact inclusive boundary — `now + 999ms` denied, `now + 1000ms` allowed — which no sleep-based test could pin reliably.

It also closed a coverage gap the injection made visible: every existing timing test used `WindowUnit::Seconds`, where `in_seconds() == 1` makes the anchor-advance multiplication `elapsed_time_units * in_seconds()` indistinguishable from `elapsed_time_units` alone. Deleting that multiplication left all tests green. Two new tests — `should_advance_the_anchor_by_the_full_unit_in_seconds` in each domain file — use `WindowUnit::Minutes` and cross the boundary twice, which fails specifically when that multiplication is missing.

The only sleep remaining in the suite is in `web_server.rs`'s `configuration` test, where time enters through the HTTP handler rather than a direct call.
