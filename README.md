# Rate Limiter Algorithms

A production-grade implementation of rate limiting algorithms in Rust, demonstrating realistic patterns used in modern web services.

## Features

- **Token Bucket Algorithm** ✓
  - Efficient token consumption and refill logic
  - Per-client independent quotas
  - Capacity-aware overflow prevention

- **Planned Algorithms**
  - Fixed Window Counter
  - Sliding Window Log
  - Sliding Window Counter

## Why Rate Limiting?

Rate limiting is essential for protecting APIs from abuse, ensuring fair resource allocation, and preventing cascading failures. This project explores multiple algorithmic approaches with realistic HTTP integration following RFC standards.

## Architecture Highlights

- **Async-first**: Built on Axum + Tokio for modern async Rust
- **Thread-safe**: Arc + Mutex for concurrent request handling
- **Configurable**: Environment-based configuration with type-safe defaults
- **Well-tested**: Separated domain logic tests and full-stack endpoint tests

## Quick Start

```bash
# Build
cargo build --release

# Run with defaults (capacity: 60 tokens/min, refill: 1 token/sec)
cargo run

# Run with custom config
BUCKET_CAPACITY=100 BUCKET_REFILL_RATE_PER_SECOND=2 cargo run

# Test
cargo test
```

Server starts on `0.0.0.0:3000`

## Example Usage

```bash
# First request (new client)
curl -H "X-API-Key: client_1" http://localhost:3000/rate-limit
# Returns: 200 with RateLimit headers

# 60th request (limit reached)
curl -H "X-API-Key: client_1" http://localhost:3000/rate-limit
# Returns: 429 Too Many Requests
```

## Documentation

- [Architecture](./ARCHITECTURE.md) — Layer design and state management
- [Domain Model](./domain-model.md) — TokenBucket behavior and invariants
- [Testing Strategy](./TESTING.md) — Unit tests vs. integration tests
- [Quick Start](./QUICKSTART.md) — Local setup and configuration
- [Architecture Decisions](./decisions/) — ADRs documenting key choices

## Future Work

- Extract rate limiting logic to reusable middleware
- Implement sliding window variants
- Add distributed rate limiting (Redis backend)
- Performance benchmarking and tuning
