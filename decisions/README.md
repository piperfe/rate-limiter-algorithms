# Architecture Decision Records (ADRs)

This directory contains significant architectural and technical decisions made during the rate limiter project.

## Index

### Framework & Ecosystem

- **[ADR-001: Web Framework Choice](./ADR-001-web-framework-choice.md)**  
  Why Axum over Actix-web, Rocket, Warp. Includes adoption data and performance comparison.

- **[ADR-003: Configuration Strategy](./ADR-003-configuration-strategy.md)**  
  Environment configuration via envy + serde with typed defaults. Adoption data for config libraries.

### System Architecture

- **[ADR-002: Concurrent State Management](./ADR-002-concurrent-access-strategy.md)**  
  Arc + Mutex for thread-safe per-client rate limiting. Includes scalability limits and alternatives considered.

- **[ADR-005: State Extraction](./ADR-005-state-extraction-with-fromref.md)**  
  Declarative #[derive(FromRef)] for handler state extraction. Trade-offs vs. manual impl.

### Testing Strategy

- **[ADR-004: Async Test Environment Variables](./ADR-004-async-test-env-vars.md)**  
  temp_env for safe, isolated env var changes in async tests. Prevents test pollution.

- **[ADR-006: Test Layer Split](./ADR-006-test-layer-split.md)**  
  Separated domain unit tests and endpoint integration tests. Rationale and scalability.

## Decision Format

Each ADR includes:
- **Context** — Why the decision was needed
- **Decision** — What we chose and why
- **Rationale** — Detailed justification
- **Trade-offs & Alternatives** — What we rejected and why
- **Implications** — How it affects future work
- **Validation** — How we verified it works
- **References** — External sources

## Status

- ✓ **Accepted** — Implemented and validated
- ⏳ **Proposed** — Under discussion
- 🗑️ **Superseded** — Replaced by newer ADR

All decisions below are **Accepted**.

## Future ADRs

Anticipated decisions for future phases:

- Rate limiting algorithm variants (fixed window, sliding window, etc.)
- Distributed rate limiting (Redis backend)
- Middleware extraction pattern
- Metrics and observability (Prometheus)
- Performance optimization (sharded locking, caching)

## How to Read These

**New to the project?**  
Start with [ADR-001](./ADR-001-web-framework-choice.md) and [ADR-002](./ADR-002-concurrent-access-strategy.md) to understand the foundation.

**Implementing new algorithms?**  
Read [ADR-006](./ADR-006-test-layer-split.md) for test structure conventions.

**Scaling to multiple instances?**  
Read [ADR-002](./ADR-002-concurrent-access-strategy.md) "Scalability Limits" section and [ADR-003](./ADR-003-configuration-strategy.md) for distributed config strategies.

**Contributing?**  
All significant decisions should have corresponding ADRs. When in doubt, ask or propose a new ADR.

## References

- [Lightweight Architecture Decision Records (AKF)](https://www.architecturekts.com/lightweight-architecture-decision-records/)
- [Documenting Architecture Decisions (Nygard)](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions.html)
