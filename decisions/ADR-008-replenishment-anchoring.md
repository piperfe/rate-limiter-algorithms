# ADR-008: Anchor Replenishment to the Period Boundary, Not the Latest Request

**Status:** Accepted
**Date:** 2026-08-25
**Author:** Rate Limiter Team

## Context

Both algorithms track a single `Instant` (`last_request_date`) and derive replenishment from the time elapsed since it. How that field is advanced determines what quota a client actually receives.

The original implementation reassigned it on every call:

```rust
self.last_request_date = now;
```

This measures the gap between *consecutive requests* rather than time since the period began, which produced two distinct defects.

**Never replenishing under sustained traffic.** A client polling every 30 seconds against a `Minutes` period computed `elapsed_units(30) == 0` on every call and was throttled indefinitely, despite requesting well below the configured rate. The clock restarted before it ever reached a boundary.

**Discarding the remainder.** Even once a boundary was crossed, jumping the anchor to `now` threw away the leftover time. `TokenBucket` at rate 1/minute, polled every 90 seconds:

| Time | elapsed | units | tokens | anchor after |
|---|---|---|---|---|
| t=90 | 90 | 1 | 1 | **90** — 30s discarded |
| t=180 | 90 | 1 | 1 | **180** — 30s discarded |

Two tokens across 180 seconds where the configured rate owes three. The shortfall is `floor(T/U)·U / T` of the intended rate, bottoming out near **50%** when the polling interval sits just under two periods. This is not an edge case: the default configuration is `Seconds`, and real HTTP traffic does not arrive on second boundaries.

## Decision

**Advance the anchor by exactly the whole periods consumed.**

```rust
let elapsed_units = self.unit_time.elapsed_units(elapsed_seconds);
let consumed = elapsed_units * self.unit_time.in_seconds();
self.last_request_date += Duration::from_secs(consumed);
```

Applied in both `src/token_bucket.rs` and `src/fixed_window.rs`.

## Rationale

### The anchor stores the remainder

Unspent time is never held in a field. It lives in the gap between the anchor and `now`, so the anchor position alone decides whether it survives:

```
new_delta = old_delta − consumed
          = (now − anchor) − consumed
          = now − (anchor + consumed)
                   └── the new anchor ──┘
```

Subtracting the consumed portion from the delta and adding it to the anchor are the same arithmetic — only a regrouped parenthesis. Setting the anchor to `now` closes the gap to zero, which is the discard.

Because only whole multiples of the period are ever added, the anchor stays congruent to its origin. Boundaries fall on a stable grid rather than re-phasing with each request.

### Consequences per algorithm

**TokenBucket** — accrual now matches the configured rate exactly. Leftover seconds carry forward and combine, so a client polling at 1.5× the period alternates between one and two tokens, averaging the configured rate. Continuous proportional accrual is the defining property of a token bucket, so this is a correctness fix rather than a preference.

**FixedWindow** — no quota is gained or lost, since a window resets the counter outright and has no remainder to carry. What changes is *when* the next reset lands. Under `= now` the grid re-phased on every reset, pushing subsequent boundaries later and making resets unpredictable from the client's side. Anchored to the boundary, a client can compute their next reset.

### Guarantee offered

> Quota replenishes on a fixed grid whose origin is the client's first request.

Note this is not wall-clock alignment — the grid starts when the bucket is created, so resets do not land at `:00` of each minute. Delivering that would require truncating the anchor to a wall-clock boundary, which neither algorithm does. If the `t` field in the `RateLimit` header is ever computed honestly (it is currently hardcoded), this grid is what it would describe.

### The gate becomes redundant

Both algorithms previously guarded with `elapsed_seconds >= unit_time.in_seconds()`. That guard existed to protect an anchor that should not have been moving. With `+=` it protects nothing — when no period has elapsed, `elapsed_units` is `0`, tokens accrued are `0`, and the anchor advances by `0`.

`FixedWindow` still needs a conditional, because its reset is all-or-nothing rather than proportional, but it is now expressed as `elapsed_units > 0`. For any `b > 0`, `a / b > 0` holds exactly when `a >= b`, so this is equivalent to the old comparison while reusing the already-tested `elapsed_units`. The boundary-equality case is therefore covered by the arithmetic tests in `src/window_unit.rs` rather than requiring a timing test.

## Trade-offs & Alternatives

**Storing an explicit carry field.** Keep `anchor = now` and track leftover seconds separately:

```rust
let total = elapsed_seconds + self.carry_seconds;
let units = self.unit_time.elapsed_units(total);
self.carry_seconds = total - (units * self.unit_time.in_seconds());
```

Behaviourally identical. Rejected because it adds a field and a second invariant to maintain, where `Instant` arithmetic already carries the remainder for free.

**Sub-period accrual.** Track fractional tokens so that half a period yields half a token. Rejected as unnecessary complexity — whole-unit accrual with a preserved remainder delivers the correct long-run rate, and the granularity is a configuration choice via `UNIT_TIME`.

**Wall-clock alignment.** Truncate the anchor to an absolute boundary so windows reset at `:00`. Rejected for now: `Instant` is monotonic and deliberately has no calendar relationship, so this would require `SystemTime` and expose the limiter to clock adjustments. Revisit only if the reset schedule is ever advertised publicly.

## Validation

Regression coverage for both defects, using `WindowUnit::Seconds` with sub-second sleeps so the whole suite stays under four seconds:

- `token_bucket::refill_and_capacity::should_not_refill_until_the_full_unit_has_elapsed`
- `token_bucket::refill_and_capacity::should_not_lose_partial_units_between_refills`
- `fixed_window::window_reset::should_not_reset_until_the_full_window_has_elapsed`
- `fixed_window::window_reset::should_not_lose_partial_units_between_window_elapses`

The partial-unit tests are the discriminating ones — they sleep 1.5 periods twice, so the carried halves must combine into a second token. Under `= now` they fail; the earlier timing tests could not catch either defect because every sleep was an exact multiple of the period.

## Implications

Any future algorithm deriving replenishment from elapsed time inherits this rule: advance the anchor by what was consumed, never to `now`. Sliding-window variants will need their own treatment, as they track a request history rather than a single anchor.

## References

- [ADR-006: Test Layer Split](./ADR-006-test-layer-split.md) — why the timing arithmetic is tested at the value-object layer
- `src/window_unit.rs` — `elapsed_units` and its tests
