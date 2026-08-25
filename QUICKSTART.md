# Quick Start

Get the rate limiter running locally in 2 minutes.

## Prerequisites

- **Rust** 1.70+ (install via [rustup](https://rustup.rs/))
- **Cargo** (included with Rust)

## Installation

```bash
# Clone the repository
git clone <repo-url>
cd rate-limiter-algorithms

# Build
cargo build

# Run (uses default config)
cargo run
```

Server listens on `http://0.0.0.0:3000`

## Configuration

Rate limiting parameters are set via environment variables:

```bash
# With defaults (capacity 60, refilling 1 token per second)
cargo run

# Override capacity
CAPACITY=100 cargo run

# Refill 2 tokens per minute instead of per second
UNIT_TIME=Minutes REFILL_RATE_PER_UNIT_TIME=2 cargo run

# All three
CAPACITY=100 UNIT_TIME=Minutes REFILL_RATE_PER_UNIT_TIME=2 cargo run
```

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `CAPACITY` | `60` | Maximum tokens in the bucket |
| `UNIT_TIME` | `Seconds` | Period the refill rate applies to — `Days`, `Hours`, `Minutes`, or `Seconds` |
| `REFILL_RATE_PER_UNIT_TIME` | `1` | Tokens added per `UNIT_TIME` |

`UNIT_TIME` is case-sensitive and must match a variant exactly. An unrecognised value fails at startup rather than falling back to the default:

```
Boot Error: ... unknown variant `seconds`, expected one of `Days`, `Hours`, `Minutes`, `Seconds`
```

Unrecognised *variable names* are ignored silently, so a typo in the variable itself leaves the default in place with no warning.

## Testing

```bash
# Run all tests
cargo test

# Run only domain tests (TokenBucket logic)
cargo test --lib token_bucket

# Run only integration tests (HTTP endpoints)
cargo test web_server::integration_tests

# Run a specific test
cargo test <test_name> -- --nocapture

# Run with output
cargo test -- --nocapture
```

## Usage Examples

### Allow a Request

```bash
# Make first request as client_1
curl -i -H "X-API-Key: client_1" http://localhost:3000/rate-limit

# Response:
# HTTP/1.1 200 OK
# RateLimit-Policy: "api-v1";q=60;w=60
# RateLimit: "api-v1";r=59;t=1;pk=:client_1:
# 
# Hello, World!
```

- `RateLimit-Policy` → server's rate limit policy (60 tokens, 60-second window)
- `RateLimit.r=59` → 59 tokens remaining after this request
- `RateLimit.pk=:client_1:` → applies to client_1

### Exceed Rate Limit

```bash
# Make 61 requests (capacity is 60)
for i in {1..61}; do
  curl -H "X-API-Key: client_1" http://localhost:3000/rate-limit
done

# After 60th request, subsequent requests return:
# HTTP/1.1 429 Too Many Requests
# RateLimit: "api-v1";r=0;t=1;pk=:client_1:
# 
# Too Many Requests
```

### Independent Clients

```bash
# Each client has independent quota
curl -H "X-API-Key: client_1" http://localhost:3000/rate-limit  # → 200
curl -H "X-API-Key: client_2" http://localhost:3000/rate-limit  # → 200

# Capacity is per-client, not global
```

## Build Variants

### Development

```bash
# Fast iteration, debug symbols
cargo build
cargo run
```

### Release

```bash
# Optimized binary for production
cargo build --release
./target/release/rate_limiter
```

## Troubleshooting

### Port Already in Use

If port 3000 is already in use, modify `src/main.rs`:

```rust
#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    // ↑ Change port here
    axum::serve(listener, create_routes()).await.unwrap();
}
```

### Boot Error on Startup

Every setting has a default, so the server runs with no env vars at all. A boot error means a variable was set to a value that could not be parsed — most often `UNIT_TIME` with the wrong casing:

```
Boot Error: ... unknown variant `seconds`, expected one of `Days`, `Hours`, `Minutes`, `Seconds`
```

Check the value against the table above. The defaults themselves live on `AppConfig` in `src/web_server.rs`.

## Next Steps

- Read [ARCHITECTURE.md](./ARCHITECTURE.md) for system design
- Check [domain-model.md](./domain-model.md) for TokenBucket algorithm details
- See [TESTING.md](./TESTING.md) for test structure
- Explore [decisions/](./decisions/) for architectural decision records
