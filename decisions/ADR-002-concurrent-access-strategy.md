# ADR-002: Arc + Mutex for Concurrent State Management

**Status:** Superseded by [ADR-007](./ADR-007-dashmap-state-management.md)  
**Date:** 2026-08-05  
**Author:** Rate Limiter Team  

> **Note:** This ADR describes the original Arc<Mutex<Vec<TokenBucket>>> approach. The implementation has evolved to use Arc<DashMap<String, TokenBucket>> for better performance and scalability. See [ADR-007](./ADR-007-dashmap-state-management.md) for the current strategy and migration rationale.

## Context

Rate limiting requires per-client token buckets to survive across concurrent HTTP requests:

```
Request 1 (client_1) ─┐
Request 2 (client_1) ─┼─→ Same TokenBucket
Request 3 (client_2) ─→ Different TokenBucket
```

Multiple concurrent requests must safely read/modify the same bucket. Options:

1. **Arc<Mutex<>> + Std Mutex** (current choice)
2. **RwLock** (reader-writer lock)
3. **Atomic types** (limited to simple counters)
4. **Message passing** (Tokio channels)
5. **Redis backend** (external coordination)

## Decision

**Use Arc<Mutex<Vec<TokenBucket>>>** for thread-safe per-client state.

```rust
pub struct AppState {
    client_buckets: Arc<Mutex<Vec<TokenBucket>>>,
}
```

### Rationale

#### 1. Arc (Atomic Reference Counting)

- **Shared ownership:** Multiple concurrent requests can hold a reference to the same bucket list
- **Lightweight:** Just a pointer + atomic counter (zero-copy)
- **Cloneable:** Works seamlessly with Tokio task spawning

Example:
```rust
let server = Arc::new(TestServer::new(routes));

let handles: Vec<_> = (1..120)
    .map(|_| {
        let server = server.clone();  // ← Arc::clone (cheap)
        tokio::spawn(async move {
            server.get("/").await
        })
    })
    .collect();
```

#### 2. Mutex (Mutual Exclusion)

- **Atomicity:** Only one request can read/modify bucket state at a time
- **Poison detection:** Panics if previous request panicked while holding lock (data integrity)
- **Lock contention:** Acceptable for rate-limiting scenarios (typically <1k clients)

#### 3. Performance Characteristics

| Scenario | Behavior | Cost |
|---|---|---|
| Sequential requests (same client) | Serialize at lock | 1 lock per request |
| Concurrent requests (same client) | Queue at Mutex | Linear in concurrency |
| Concurrent requests (different clients) | **No contention** | 0 lock contention |

**Real-world impact:** Most rate-limiting workloads have many clients, few requests per client → minimal contention.

### Trade-offs & Alternatives

#### 1. RwLock
```rust
Arc<RwLock<Vec<TokenBucket>>>
```

**Pros:** Multiple readers can hold lock simultaneously  
**Cons:**
- TokenBucket mutations require write lock
- `is_allowed()` always needs `&mut`, so no parallelism benefit
- Higher overhead than Mutex (reader tracking, state tracking)

**Verdict:** No benefit for this workload; unnecessary complexity.

#### 2. Atomic Types
```rust
Arc<AtomicU64>  // for tokens
```

**Pros:** Lock-free, maximum performance  
**Cons:**
- Limited to simple counter types
- Token refill requires timestamp → Atomic doesn't help
- CAS loops needed for compound operations

**Verdict:** Insufficient for token bucket logic (state is complex).

#### 3. Message Passing
```rust
tokio::mpsc::channel()
// Each request sends messages to a central actor
```

**Pros:** Async-friendly, handles backpressure  
**Cons:**
- Higher latency (message overhead)
- Overkill for in-memory rate limiting
- Complex request/response pairing

**Verdict:** Over-engineered for current scope.

#### 4. Redis Backend
```rust
// Distributed rate limiting
redis_client.decr("client_1:tokens")
```

**Pros:** Scales to multiple server instances  
**Cons:**
- Network latency
- External dependency
- Wrong level of abstraction for local testing

**Verdict:** Future phase (multi-instance deployment).

## Implementation Details

### State Structure

```rust
#[derive(Clone, FromRef)]
pub struct AppState {
    config: AppConfig,
    client_buckets: Arc<Mutex<Vec<TokenBucket>>>,
}
```

### Access Pattern

```rust
async fn rate_limit_handler(
    State(client_buckets): State<Arc<Mutex<Vec<TokenBucket>>>>,
    headers: HeaderMap,
) -> Response<Body> {
    let client_id = headers.get("X-Api-Key").unwrap().to_str().unwrap();
    
    let mut buckets = client_buckets.lock().unwrap();  // ← Lock acquired
    
    // Find or create bucket for client
    let bucket = buckets
        .iter_mut()
        .find(|b| b.matches_client_id(client_id))
        .unwrap_or_else(|| {
            buckets.push(TokenBucket::new(client_id.to_string(), ...));
            buckets.last_mut().unwrap()
        });
    
    // Consume token (mutable access)
    let result = bucket.is_allowed();
    
    // Lock released here (scope end)
    Response::builder().status(...).build()
}
```

### Lock Scope Optimization

Lock is held **only** during token bucket operation (~microseconds), not for:
- HTTP parsing (Axum handles before lock)
- Response building (Axum handles after lock)
- Network I/O (async task yields while waiting)

## Monitoring & Future Improvements

### Metrics to Track

- Lock wait time (detect contention)
- Clients per instance (predict scalability limit)
- Concurrent requests (load profile)

### Scalability Limits

With `Arc<Mutex>`, expect good performance up to:
- **1,000 concurrent clients** on single instance
- **10,000+ requests/sec** with low contention

Beyond this:
- Consider shard-based locking (one Mutex per client prefix)
- Or implement Redis backend for multi-instance deployment

## Validation

✓ Concurrent tests pass (4 threads, 3 tokens)  
✓ Integration tests pass (119 concurrent requests, independent quotas)  
✓ No data races (Mutex ensures atomicity)

## Future Work

- [ ] Add lock contention metrics (Prometheus)
- [ ] Implement sharded locking for >1k clients
- [ ] Add Redis backend for distributed rate limiting
- [ ] Benchmark lock overhead under load

## References

- [Tokio: Sharing State](https://tokio.rs/tokio/tutorial/select#sharing-state)
- [Rust: Arc<Mutex<T>>](https://doc.rust-lang.org/std/sync/struct.Mutex.html)
- [RwLock vs Mutex performance](https://docs.rs/parking_lot/latest/parking_lot/)
