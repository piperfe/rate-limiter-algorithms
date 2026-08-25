# Domain Model

The domain layer holds the rate-limiting algorithms and the value object they share. It knows nothing about HTTP, configuration, or storage — see [ARCHITECTURE.md](./ARCHITECTURE.md) for how it is wired.

Field-level detail lives in the source. This document covers the concepts, the invariants, and the reasoning that is not visible from a struct definition.

## WindowUnit (`src/window_unit.rs`)

A period — days, hours, minutes, or seconds — with two operations:

- **`in_seconds()`** — how long one period lasts
- **`elapsed_units(elapsed_seconds)`** — how many *whole* periods fit into a span, discarding any remainder

Both algorithms derive their timing from `elapsed_units`, so replenishment is granted only for complete periods. Partial time is never lost, but it is not spendable until it completes a period — see *Anchoring* below.

This is the seam that keeps the algorithms testable without sleeping. Verifying that a `Minutes` bucket waits a full minute is arithmetic, not a minute-long test.

## Anchoring — shared by both algorithms

Each algorithm stores a single `Instant` marking where the current period began. Elapsed time is measured from that anchor, and **the anchor advances by whole periods consumed — never to `now`**:

```
anchor += elapsed_units × unit_time.in_seconds()
```

The unspent remainder is not stored in a field. It lives in the gap between the anchor and `now`, so leaving the anchor on the period boundary is what preserves it. Setting it to `now` closes that gap and discards the time — which under-delivers quota by up to 50% and, under sustained sub-period traffic, prevents replenishment entirely.

This is the single most important invariant in the domain layer. [ADR-008](./decisions/ADR-008-replenishment-anchoring.md) has the derivation, the worked examples, and the alternatives considered. Any future algorithm deriving replenishment from elapsed time inherits this rule.

Because only whole multiples of the period are ever added, the anchor stays congruent to its origin: periods fall on a stable grid whose zero point is the bucket's creation. Note this is *not* wall-clock alignment — resets do not land on `:00` of each minute.

## TokenBucket (`src/token_bucket.rs`)

Tokens **accrue continuously** at `refill_rate_per_unit_time` per `unit_time`, up to `capacity`. Each allowed request spends one.

Invariants:

1. `remaining_tokens <= capacity` — accrual is clamped, so idle time cannot bank unlimited quota
2. The long-run grant rate equals the configured rate — this is what the anchoring rule protects
3. The anchor only moves forward

Continuous proportional accrual is what makes this a token bucket rather than a fixed window. A client that has been quiet accumulates tokens gradually and can spend them in a burst up to `capacity`.

**Zero rate** is legal: the bucket never replenishes, so the first `capacity` requests are allowed and everything after is denied.

**Overflow** in the accrual multiply is documented on `TokenBucket::new` — it requires a configured rate around 5.8×10¹¹ tokens/second, so the arithmetic is deliberately unguarded.

## FixedWindow (`src/fixed_window.rs`)

The counter **resets outright** to `capacity` once a full period elapses. Nothing accrues in between.

Invariants:

1. `remaining_tokens <= capacity`
2. Within a period, only denials and decrements happen — no replenishment
3. The anchor only moves forward

Capping is **not** a concept here. A token bucket must clamp because tokens accumulate; a fixed window jumps straight to `capacity`, so there is nothing to overflow. Do not port a capacity-capping test from one to the other.

The guarantee offered is *"quota resets every period on a fixed grid"* rather than *"quota resets one period after your last reset."* The anchoring rule is what makes the reset schedule predictable to a client instead of re-phasing on every request.

## Time source

Both algorithms use `std::time::Instant`, which is **monotonic**. System clock adjustments — NTP steps, daylight saving, manual changes — cannot move it backwards or affect elapsed calculations. The trade-off is that `Instant` has no calendar relationship, which is why wall-clock-aligned windows are not currently possible (see ADR-008).

Both call `Instant::now()` internally today. A TODO in each file proposes taking `now: Instant` as a parameter instead, so timing tests can use exact offsets rather than `thread::sleep`.

## Client identity

Neither algorithm stores a client identifier. Buckets are keyed by API key in the `DashMap` that owns them, so identity is the storage layer's concern — see [ADR-007](./decisions/ADR-007-dashmap-state-management.md). Each instance is single-client by construction and is not thread-safe on its own; atomicity comes from the map's per-entry locking.

## Testing

See [TESTING.md](./TESTING.md) for which layer owns which behaviour and why the time arithmetic is tested separately from the algorithms.
