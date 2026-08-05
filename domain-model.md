# Domain Model: Token Bucket

This document describes the TokenBucket algorithm, its state invariants, and behavior.

## Overview

The token bucket is a rate limiting algorithm where:
- Tokens accumulate at a fixed rate (`refill_rate_per_second`)
- Each request consumes one token
- Requests are allowed only if tokens are available
- Bucket capacity caps the maximum tokens (prevents unbounded accumulation)

## State

```rust
pub struct TokenBucket {
    client_id: String,                    // Unique identifier
    capacity: u64,                        // Max tokens (e.g., 60)
    remaining_tokens: u64,                // Current tokens
    refill_rate_per_second: u64,          // Tokens added per second
    last_request_date: Instant,           // Timestamp of last operation
}
```

## Invariants

1. **Capacity Bound:** `remaining_tokens <= capacity` always
2. **Non-negative:** `remaining_tokens >= 0` always
3. **Client Isolation:** Each TokenBucket owns exactly one client's quota
4. **Time Monotonicity:** `last_request_date` only moves forward

## Operations

### `is_allowed() -> AllowedTokenRequest`

**Algorithm:**
```
1. Calculate elapsed time since last request
2. Calculate refilled tokens = elapsed_seconds * refill_rate_per_second
3. Calculate available = min(refilled + remaining, capacity)
4. Update remaining_tokens = available
5. Update last_request_date = now
6.
7. If available == 0:
     return AllowedTokenRequest { allowed: false, remaining_tokens: 0 }
8. Else:
     remaining_tokens -= 1
     return AllowedTokenRequest { allowed: true, remaining_tokens }
```

**Time Complexity:** O(1)

**Examples:**

_Initial state: capacity=60, rate=1 token/sec, remaining=60_

**Request 1 (t=0s):**
- Elapsed: 0s → refilled: 0
- Available: min(0 + 60, 60) = 60
- Remaining after consume: 59
- **Result:** allowed=true, remaining=59

**Request 2 (t=0.1s):**
- Elapsed: 0.1s → refilled: 0 (rounds down to 0 seconds)
- Available: min(0 + 59, 60) = 59
- **Result:** allowed=true, remaining=58

**Request 61 (t=0s, after 60 requests):**
- Elapsed: ~0s → refilled: 0
- Available: min(0 + 0, 60) = 0
- **Result:** allowed=false, remaining=0

**Request 62 (t=2s, after 60 initial requests):**
- Elapsed: 2s → refilled: 2
- Available: min(2 + 0, 60) = 2
- Remaining after consume: 1
- **Result:** allowed=true, remaining=1

## Client Identification

Each TokenBucket is uniquely identified by `client_id` (typically from HTTP `X-API-Key` header).

**Matching:**
```rust
pub fn matches_client_id(&self, client_id: &str) -> bool {
    self.client_id == client_id
}
```

## Concurrency Behavior

TokenBucket itself is not thread-safe (interior mutability not used). Thread safety is handled by the caller via `Arc<Mutex<Vec<TokenBucket>>>`:

- Multiple concurrent requests for the **same client** serialize at the Mutex (one request processes, others wait)
- Requests for **different clients** access different buckets (no lock contention)

**Trade-off:** Simple implementation, acceptable for typical rate-limiting scenarios (hundreds of clients, not thousands).

## Edge Cases

### Capacity Reached

Once bucket is full, no further refill occurs:
```
remaining = min(0 + refilled, capacity)
```
If refilled is large, it's capped at capacity.

### Time Drift

If `last_request_date` is far in the past (e.g., system clock adjustment), refill calculates correctly:
```
refilled = elapsed * rate
```
Capped at capacity, so bucket never exceeds limit.

### Zero Rate

If `refill_rate_per_second = 0`, bucket never refills. Behavior:
- First N requests allowed (consume N tokens)
- All subsequent requests denied (capacity depleted)

## Testing Strategy

See [TESTING.md](./TESTING.md) for unit test scenarios covering:
- Token consumption
- Token refill
- Capacity overflow prevention
- Concurrent access (thread safety)
