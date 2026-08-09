# Architecture

This document describes the system design, layer boundaries, and key decisions.

## System Layers

```
┌─────────────────────────────────────────┐
│  HTTP Layer (Axum routes)               │
│  - Extracts X-API-Key header            │
│  - Returns 200 (allowed) or 429 (denied)│
│  - RFC 7231 rate-limit headers          │
└──────────────┬──────────────────────────┘
               │ State extraction (FromRef)
┌──────────────▼──────────────────────────┐
│  Domain Layer (TokenBucket)             │
│  - Token consumption logic              │
│  - Refill calculation                   │
│  - Concurrency-safe via DashMap        │
│  - Per-client independent quotas        │
└─────────────────────────────────────────┘
```

## State Management

**Per-Client Isolation:**
```rust
Arc<DashMap<String, TokenBucket>>
  └─ Each TokenBucket owns one client's quota (keyed by client_id)
     └─ Arc: shared ownership across requests
     └─ DashMap: concurrent HashMap with O(1) lookups and atomic operations
```

**Configuration:**
```rust
struct AppState {
    config: AppConfig,                                   // Loaded from env at startup
    client_buckets: Arc<DashMap<String, TokenBucket>>   // Grows as new clients appear
}
```

### Why DashMap?

- **O(1) Lookup** by client_id (vs. O(n) Vec scan)
- **Concurrent Writes** handled automatically without explicit locking per operation
- **Atomic Entry API** (`entry().or_insert_with()`) prevents race conditions
- **Lower Contention** than Arc<Mutex<Vec>> for workloads with many clients
- See [ADR-007](./decisions/ADR-007-dashmap-state-management.md) for detailed justification and migration from Arc+Mutex

## Configuration

### Environment Variables

Loaded at startup via `envy` + `serde`:

```bash
BUCKET_CAPACITY=60                      # Max tokens in bucket
BUCKET_REFILL_RATE_PER_SECOND=1        # Tokens added per second
```

Defaults provided via `#[serde(default)]` — no env vars required for local development.

### Why envy + serde?

- Typed deserialization (fails fast if config invalid)
- Serde integration (standard Rust ecosystem choice)
- Paired with `#[serde(default)]` for optional env vars
- See [ADR-003](./decisions/ADR-003-config-strategy.md)

## Request Flow

```
Client Request (X-API-Key: client_1)
    │
    ├─ Extract API key from header
    │
    ├─ Look up or create TokenBucket for client
    │
    ├─ Call is_allowed() on bucket
    │    ├─ Calculate tokens refilled since last request
    │    ├─ Cap at capacity
    │    ├─ Consume 1 token (if available)
    │    └─ Return { allowed, remaining_tokens }
    │
    └─ Return 200 (allowed) or 429 (denied)
       └─ Include RFC rate-limit headers
```

## HTTP Response Headers

**RateLimit-Policy:** (server capability)
```
"api-v1";q=1;w=1
  └─ q: quota (tokens per window)
  └─ w: window (in seconds)
```

**RateLimit:** (per-client state)
```
"api-v1";r=59;t=1;pk=:client_1:
  └─ r: remaining tokens
  └─ t: time window (seconds)
  └─ pk: per-key identifier (client ID)
```

## Testing Strategy

Two-tier testing ensures both correctness and integration:

1. **Domain Unit Tests** (`src/token_bucket.rs`)
   - Test TokenBucket logic in isolation
   - Verify token consumption, refill, and concurrency safety
   - No HTTP involved

2. **Endpoint Integration Tests** (`src/web_server.rs`)
   - Test full HTTP request/response cycle
   - Verify multi-client independence
   - Verify rate-limit header format
   - Include concurrent request scenarios

See [TESTING.md](./TESTING.md) for detailed conventions.

## Future Extensibility

### Middleware Pattern

Current implementation: per-route handler. Future: extract to Tower middleware for composability.

```rust
// Future API
Router::new()
    .route("/api/users", ...)
    .layer(RateLimitMiddleware::new(
        RateLimiter::token_bucket(capacity: 60, rate: 1)
    ))
```

### Algorithm Variants

TokenBucket logic isolated in `src/token_bucket.rs`. New algorithms (fixed window, sliding window) will be separate structs implementing a common trait:

```rust
trait RateLimitAlgorithm {
    fn is_allowed(&mut self) -> AllowedTokenRequest;
}
```

### Distributed Rate Limiting

Per-client bucket state currently in-memory (Arc<DashMap>). Future: extract to pluggable backend (Redis, etc.) for multi-instance deployments. See [ADR-007](./decisions/ADR-007-dashmap-state-management.md) for scalability analysis.
