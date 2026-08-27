# ADR-009: Clock Injection via Explicit `Instant` Parameter

**Status:** Accepted  
**Date:** 2026-08-26  
**Author:** Rate Limiter Team

## Context

`TokenBucket` and `FixedWindow` originally called `Instant::now()` internally, in both their constructors and `is_allowed`. Time was a hidden input: the only way to test time-dependent behaviour was to actually wait.

This had two costs.

**Slow, imprecise tests.** Verifying a `Minutes` refill required `thread::sleep`, tolerating whatever jitter the scheduler introduced. Two 60-second tests put the domain suite at 60.01s (see [ADR-006](./ADR-006-test-layer-split.md)), and even after that was reduced to seconds-scale sleeps, every timing assertion carried roughly ±500ms of overshoot risk — stable locally, fragile on a loaded CI runner.

**Unreachable boundary assertions.** Whether a period boundary is inclusive or exclusive — does `elapsed == unit_time` count as elapsed? — could not be pinned down by sleeping. `sleep(1000ms)` reliably delivers *at least* 1000ms, never exactly 1000ms, so the single instant where the answer changes was untestable.

## Decision

**Take `now: Instant` as an explicit parameter** in `TokenBucket::new`, `TokenBucket::is_allowed`, `FixedWindow::new`, and `FixedWindow::is_allowed`, rather than calling `Instant::now()` internally.

```rust
pub fn new(capacity: u64, unit_time: WindowUnit, refill_rate_per_unit_time: u64, now: Instant) -> Self
pub fn is_allowed(&mut self, now: Instant) -> AllowedTokenRequest
```

`web_server.rs` reads the clock once per request and passes the same `now` to both the bucket lookup and `is_allowed`, so a newly created bucket's anchor is exactly the instant it was first evaluated:

```rust
let now = Instant::now();
let mut client_bucket = client_buckets.entry(key).or_insert_with(|| TokenBucket::new(..., now));
let response = client_bucket.is_allowed(now);
```

This is functional core / imperative shell: the domain layer becomes a pure function of its inputs, and the one real `Instant::now()` call lives at the edge.

## Rationale

### Tests become exact and instant

`Instant + Duration` is valid even though an arbitrary `Instant` can't be constructed, so tests capture one origin and derive every subsequent moment from it:

```rust
let now = Instant::now();
let mut bucket = TokenBucket::new(5, WindowUnit::Seconds, 1, now);
bucket.is_allowed(now + Duration::from_millis(999));   // denied
bucket.is_allowed(now + Duration::from_millis(1000));  // allowed — boundary is inclusive
```

No sleeping, no jitter. The domain suite (`token_bucket` + `fixed_window`) dropped from 3.01s to 0.00s. Boundary assertions that were previously unreachable are now a single line.

### A coverage blind spot became visible, and was closed

Every domain timing test used `WindowUnit::Seconds`, where `in_seconds() == 1`. The anchor-advance line

```rust
self.last_request_date += Duration::from_secs(elapsed_time_units * self.unit_time.in_seconds());
```

is `× 1` under `Seconds` — indistinguishable from omitting the multiplication entirely. Deleting `* self.unit_time.in_seconds()` from both algorithms left all 31 existing tests green. This is the same shape of defect [ADR-008](./ADR-008-replenishment-anchoring.md) fixed: masked by the one configuration every test happened to use, including the default.

Closed by one test per algorithm using `WindowUnit::Minutes` and crossing the boundary twice — `should_advance_the_anchor_by_the_full_unit_in_seconds` in both `token_bucket.rs` and `fixed_window.rs`. Deleting the multiplication now fails exactly those two tests and nothing else, confirmed by reintroducing the sabotage.

### A latent caller bug surfaced during the refactor

While threading `now` through, `WindowUnit`'s elapsed-time helper briefly took two `Instant`s:

```rust
pub fn elapsed_time_units(self, t1: Instant, t2: Instant) -> u64
```

The two call sites passed them in **opposite order** — `fixed_window.rs` called it as `(anchor, now)`, `token_bucket.rs` as `(now, anchor)`. `Instant::duration_since` saturates to zero on a backwards subtraction rather than panicking (since Rust 1.60), so the reversed call silently computed `duration_since(later, earlier) = 0` on every call. `TokenBucket` never replenished. No panic, no test failure — every domain timing test used `Seconds`, and the value-object tests for `elapsed_time_units` only exercised one argument order, so they passed regardless of what any caller did.

**Fix:** the helper takes a single `Duration` instead of two `Instant`s:

```rust
pub fn elapsed_time_units(self, elapsed: Duration) -> u64 {
    elapsed.as_secs() / self.in_seconds()
}
```

```rust
let elapsed = now.duration_since(self.last_request_date);
let elapsed_time_units = self.unit_time.elapsed_time_units(elapsed);
```

This isn't just a fix — it's a structurally stronger signature. Two same-typed parameters can be transposed by any caller with nothing to catch it; a single `Duration`, computed at the call site where the direction is visible, can't be swapped because there's nothing left to swap. The lesson generalizes: **a unit test on a helper proves the helper is correct, never that it's used correctly.** The parameter-order contract is between the function and its callers, and no test of the function in isolation can verify it — only a type that makes the wrong order unrepresentable, or a test of the caller, can.

## Trade-offs & Alternatives

**A `Clock` trait, injected as a dependency.**
```rust
trait Clock { fn now(&self) -> Instant; }
```
**Pros:** conventional DI pattern, mockable.  
**Cons:** every call site needs a trait object or generic parameter; adds an abstraction for a single implementation. `Instant` is already a value type — passing it directly needs no trait.  
**Verdict:** unnecessary indirection for this domain.

**Leave `Instant::now()` internal, accept sleep-based tests.**  
**Pros:** no API change.  
**Cons:** this is the status quo the Context section describes — slow, jitter-prone, boundary-blind, and it's what let the `Seconds`-only blind spot and the parameter-order bug both ship undetected.  
**Verdict:** rejected; it's the problem, not a solution.

**Only inject at the endpoint test layer, leave domain tests sleeping.** Considered mid-refactor. Rejected because the domain layer is exactly where the boundary and blind-spot bugs live — pushing injection only to the edge would have fixed the API ergonomics without fixing the actual testability gap.

## Implications

Any new rate-limiting algorithm takes `now: Instant` the same way. The one remaining sleep in the suite is in `web_server.rs`'s `configuration` test (`should_apply_the_configured_unit_time_to_the_refill_period`), where time enters through the HTTP handler rather than a direct call — injecting there would require threading a clock through `AppState`, a larger change deferred as out of scope for this ADR.

## Validation

- `token_bucket` and `fixed_window` domain suites: 0.00s (was 3.01s / 1.01s)
- Full suite: 1.12–1.14s across repeated runs (was 3.01s), with the residual time entirely in the one HTTP-layer sleep
- Boundary tests added: `999ms → denied`, `1000ms → allowed`, both algorithms
- Sabotage-verified: reverting the anchor to `= now`, dropping the capacity clamp, reversing the elapsed-time direction, and deleting the `Minutes` multiplier each fail a distinct, expected set of tests

## References

- [ADR-006: Test Layer Split](./ADR-006-test-layer-split.md) — the value-object layer this decision builds on
- [ADR-008: Replenishment Anchoring](./ADR-008-replenishment-anchoring.md) — the anchor-advance invariant this decision made testable
- `src/window_unit.rs` — `elapsed_time_units` and its tests
