# Rate Limiter Algorithms

A production-grade implementation of rate limiting algorithms in Rust, demonstrating realistic patterns used in modern web services.

## Features

- **Token Bucket Algorithm** ✓
  - Continuous token accrual at a configurable rate and period
  - Per-client independent quotas
  - Capacity-aware overflow prevention

- **Fixed Window Counter** ✓
  - Quota resets to full capacity on a stable period grid
  - Shares the time-unit handling and anchoring rules with Token Bucket

- **Planned Algorithms**
  - Sliding Window Log
  - Sliding Window Counter

## Why Rate Limiting?

Rate limiting is essential for protecting APIs from abuse, ensuring fair resource allocation, and preventing cascading failures. This project explores multiple algorithmic approaches with realistic HTTP integration, following the IETF `RateLimit` header field conventions.

## Architecture Highlights

- **Async-first**: Built on Axum + Tokio for modern async Rust
- **Thread-safe**: DashMap (concurrent HashMap) for lock-free per-client state
- **Configurable**: Environment-based configuration with type-safe defaults
- **Well-tested**: Separated domain logic tests and full-stack endpoint tests with organized test structure

## Quick Start

```bash
# Build
cargo build --release

# Run with defaults (capacity 60, refilling 1 token per second)
cargo run

# Run with custom config
CAPACITY=100 UNIT_TIME=Minutes REFILL_RATE_PER_UNIT_TIME=2 cargo run

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
