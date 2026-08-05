# ADR-003: Environment Configuration via envy + serde

**Status:** Accepted  
**Date:** 2026-08-05  
**Author:** Rate Limiter Team

## Context

Rate limiter behavior (capacity, refill rate) must be configurable per deployment:
- Development: low limits for testing (capacity=10)
- Staging: moderate limits (capacity=60)
- Production: high limits (capacity=1000)

Configuration must be:
- **Type-safe** (fail fast if invalid)
- **Environment-based** (12-factor app)
- **Defaulted** (work without env vars for local dev)
- **Deserialized** (string → typed struct)

Options evaluated:
1. **envy + serde** (current choice)
2. **dotenv + serde_json**
3. **config crate** (toml/json files)
4. **Manual env::var() + parsing**
5. **System environment only** (no defaults)

## Adoption Data (2026)

| Library | Downloads/mo | Usage Pattern |
|---|---|---|
| **envy** | 10M+ | Typed env → Struct |
| **config** | 8M+ | File-based config |
| **dotenv** | 3M+ | .env file loading |
| **serde** | 350M+ | **De facto standard** |

**Key insight:** envy + serde is the Rust ecosystem default for typed environment configuration.

## Decision

**Use envy + serde for application configuration.**

```rust
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
struct AppConfig {
    #[serde(default = "default_bucket_capacity")]
    bucket_capacity: u64,
    
    #[serde(default = "default_bucket_rate")]
    bucket_refill_rate_per_second: u64,
}

fn default_bucket_capacity() -> u64 { 60 }
fn default_bucket_rate() -> u64 { 1 }

// Load at startup
let config = envy::from_env::<AppConfig>()
    .expect("Failed to parse configuration");
```

### Rationale

#### 1. Type Safety

Without typing:
```bash
# Operator sets wrong value
export BUCKET_CAPACITY=abc
```
Silent failure or runtime error. With envy + serde:

```rust
envy::from_env::<AppConfig>()
// ↑ Fails immediately with clear error
// Error: invalid digit found in string for "bucket_capacity"
```

#### 2. Serde Ecosystem

Serde is the de facto Rust serialization standard:
- 350M+ monthly downloads
- Battle-tested, production-grade
- Works with every format (JSON, TOML, YAML, env)
- Extensible via custom derives

Benefits:
- Custom deserialization logic if needed
- Validation via serde(validate)
- Type coercion (string → u64)

#### 3. Defaults via #[serde(default)]

Allows optional environment variables:

```bash
# Works without any env vars (uses defaults)
cargo run

# Override capacity, use default rate
BUCKET_CAPACITY=100 cargo run

# Override both
BUCKET_CAPACITY=100 BUCKET_REFILL_RATE_PER_SECOND=2 cargo run
```

Without defaults, production would be blocked if a single env var is missing.

#### 4. envy Library

**Why envy over manual parsing?**

Manual approach:
```rust
let capacity = std::env::var("BUCKET_CAPACITY")
    .map(|s| s.parse::<u64>().expect("invalid"))
    .unwrap_or(60);

let rate = std::env::var("BUCKET_REFILL_RATE_PER_SECOND")
    .map(|s| s.parse::<u64>().expect("invalid"))
    .unwrap_or(1);
```

envy approach:
```rust
let config = envy::from_env::<AppConfig>()?;
```

Benefits:
- **DRY:** Single struct definition, no duplication
- **Composability:** Works with serde's ecosystem
- **Maintainability:** Adding new config fields is trivial
- **Error messages:** Clear what failed

### Trade-offs & Alternatives

#### 1. config Crate
```rust
let config = config::Config::builder()
    .add_source(config::File::with_name("config/default"))
    .build()?
    .try_deserialize::<AppConfig>()?;
```

**Pros:** Multi-source (files + env), hierarchical  
**Cons:**
- Overkill for 2 env vars
- Requires separate config files
- Higher startup cost

**Verdict:** Over-engineered for current scope; better for complex hierarchical config (10+ vars).

#### 2. dotenv + Manual Parsing
```rust
dotenv::dotenv().ok();
let capacity = std::env::var("BUCKET_CAPACITY")?.parse()?;
```

**Pros:** Simple, explicit  
**Cons:**
- Manual parsing error-prone
- No type checking
- .env files not portable (CI/CD typically use direct env vars)

**Verdict:** .env useful for local development, but dotenv alone insufficient; envy provides typing on top.

#### 3. System Environment Only (No Defaults)
```rust
let config = envy::from_env::<AppConfig>()?;
// ↑ Fails if any var missing
```

**Pros:** Explicit configuration, no surprises  
**Cons:**
- Production deployments must set all vars (operational overhead)
- Local development broken (verbose setup)
- Forces copy-paste in CI/CD

**Verdict:** Defaults essential for developer ergonomics; serde(default) enables both safety and convenience.

#### 4. Async Configuration (Config Reloading)
```rust
// Reload config every 5 seconds without restart
Arc<tokio::sync::RwLock<AppConfig>>
```

**Pros:** Hot config updates  
**Cons:**
- Complexity (multi-version bucket states)
- Rate limiting is stateful per client (can't change capacity mid-stream)

**Verdict:** Out of scope; static config loaded once at startup is appropriate.

## Implementation

### Configuration Struct

```rust
#[derive(Deserialize, Clone, Debug)]
struct AppConfig {
    #[serde(default = "default_bucket_capacity")]
    bucket_capacity: u64,
    
    #[serde(default = "default_bucket_rate")]
    bucket_refill_rate_per_second: u64,
}

fn default_bucket_capacity() -> u64 { 60 }
fn default_bucket_rate() -> u64 { 1 }
```

### Startup Loading

```rust
#[tokio::main]
async fn main() {
    let app_config = envy::from_env::<AppConfig>()
        .expect("Boot Error: Required environment variables misconfigured!");
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, create_routes_from_env()).await.unwrap();
}
```

### Testing with temp_env

Tests override env vars safely:

```rust
#[tokio::test]
async fn test_custom_capacity() {
    let vars = [("BUCKET_CAPACITY", Some("10"))];
    
    temp_env::async_with_vars(vars, (|| async {
        let config = envy::from_env::<AppConfig>()?;
        assert_eq!(config.bucket_capacity, 10);
    })()).await;
}
```

## Adding New Configuration

To add a new config parameter:

```rust
#[derive(Deserialize, Clone, Debug)]
struct AppConfig {
    #[serde(default = "default_bucket_capacity")]
    bucket_capacity: u64,
    
    #[serde(default = "default_bucket_rate")]
    bucket_refill_rate_per_second: u64,
    
    // New field
    #[serde(default = "default_request_timeout")]
    request_timeout_ms: u64,
}

fn default_request_timeout() -> u64 { 5000 }
```

No changes to parsing logic needed; envy handles new field automatically.

## Validation

✓ `envy::from_env()` called at startup  
✓ Defaults applied via `#[serde(default)]`  
✓ Tests use `temp_env::async_with_vars` for isolation  
✓ Production works without env vars set (uses defaults)

## Future Enhancements

- [ ] Add validation layer (min/max capacity checks)
- [ ] Support TOML config files (config crate) for complex deployments
- [ ] Environment-specific profiles (dev.toml, prod.toml)
- [ ] Config schema documentation (serde defaults)

## References

- [envy: Deserialize environment variables into typed structs](https://docs.rs/envy/)
- [serde: Framework for serialization](https://serde.rs/)
- [12 Factor App: Configuration](https://12factor.net/config)
- [Rust: Environment Variables](https://doc.rust-lang.org/std/env/fn.var.html)
