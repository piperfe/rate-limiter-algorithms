# ADR-001: Axum as Primary Web Framework

**Status:** Accepted  
**Date:** 2026-08-05  
**Author:** Rate Limiter Team

## Context

Building a rate limiting service requires:
- High-performance async HTTP handling
- Type-safe request/response routing
- Composable middleware support
- Strong ecosystem integration
- Production-grade maturity

Evaluated frameworks:
- **Axum** (Tokio team)
- **Actix-web** (actor-based)
- **Rocket** (developer ergonomics)
- **Warp** (functional composition)

## Adoption Data (2026)

| Framework | Downloads/mo | GitHub Stars | Use Case |
|---|---|---|---|
| **Axum** | 87M+ | 25,000+ | **Ecosystem standard** |
| Actix-web | 75M+ | 22,000+ | Throughput-critical |
| Rocket | 5M+ | 24,500+ | Rapid prototyping |
| Warp | 3M+ | 9,000+ | Functional composition |

**Key Metric:** Axum leads in recent adoption (87M downloads) and is the official Tokio ecosystem recommendation.

## Decision

**Choose Axum** as the primary HTTP framework.

### Rationale

1. **Ecosystem Integration**
   - Built by Tokio team
   - Seamless async/await integration
   - Tower middleware compatibility (standard middleware ecosystem)
   - 100M+ monthly tokio downloads indicate mature ecosystem

2. **Adoption & Community**
   - 25,000+ GitHub stars (largest Rust web framework community)
   - 87M+ monthly downloads (2.3x Actix-web, 17x Rocket)
   - Industry consensus for new projects in 2026

3. **Developer Experience**
   - Type-safe routing
   - Declarative macros (`derive(FromRef)`)
   - Excellent error messages
   - Batteries-included but not opinionated (lightweight layer)

4. **Performance**
   - Comparable to Actix-web on TechEmpower benchmarks
   - Single-threaded async → excellent for rate-limiting workloads
   - Lower CPU usage than actor-based frameworks

5. **Extensibility**
   - Tower middleware pattern proven in production
   - Easy to compose layers (logging, metrics, auth)
   - Clear path to distributed rate limiting (middleware)

### Trade-offs

- **Actix-web:** Wins raw throughput benchmarks, but higher complexity (actor model)
  - *Verdict:* Not needed for rate-limiting logic (low CPU cost)

- **Rocket:** Better DX for rapid prototyping, but smaller ecosystem
  - *Verdict:* Ecosystem constraints outweigh ergonomics for this project

## Alternatives Considered & Rejected

1. **Actix-web** — Actor model unnecessary; Axum's simpler concurrency model adequate
2. **Rocket** — Smaller ecosystem; less suitable for long-term maintenance
3. **Warp** — Functional composition makes state management harder (rate limiting = mutable state)

## Implications

1. **Dependencies:** `axum = "0.8"`, `tokio = "1"` (already standard)
2. **Middleware Path:** Can leverage Tower ecosystem for future extraction
3. **State Management:** Axum's `with_state()` and `FromRef` enable clean state passing
4. **Learning Curve:** Team familiar with Tokio ecosystem—low onboarding cost

## Validation

✓ Resolved: Axum used throughout `src/web_server.rs`  
✓ Works well with: `axum-macros`, `axum-test`  
✓ Scales to: Rate-limiting middleware pattern

## References

- [Axum GitHub](https://github.com/tokio-rs/axum)
- [Tokio Ecosystem](https://tokio.rs/)
- [Rust Web Frameworks in 2026: Comparison](https://reintech.io/blog/axum-vs-actix-web-vs-rocket-rust-framework-comparison-2026)
