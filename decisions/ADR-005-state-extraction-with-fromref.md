# ADR-005: Declarative State Extraction via axum-macros::FromRef

**Status:** Accepted  
**Date:** 2026-08-05  
**Author:** Rate Limiter Team

## Context

Axum handlers need access to application state:

```rust
async fn rate_limit_handler(
    State(config): State<AppConfig>,
    State(client_buckets): State<Arc<DashMap<String, TokenBucket>>>,
) -> Response<Body> {
    // Use config and client_buckets
}
```

But `State` extracts the entire `AppState`. Options:

1. **Manual impl FromRef** (boilerplate)
2. **axum-macros #[derive(FromRef)]** (declarative)
3. **Pass entire AppState** (no extraction)
4. **Tower layers** (separate state per layer)

## Decision

**Use #[derive(FromRef)] from axum-macros for declarative field extraction.**

```rust
#[derive(Clone, FromRef)]
pub struct AppState {
    config: AppConfig,
    client_buckets: Arc<DashMap<String, TokenBucket>>,
}

// Handlers can now extract individual fields:
async fn rate_limit_handler(
    State(config): State<AppConfig>,
    State(buckets): State<Arc<DashMap<String, TokenBucket>>>,
) -> Response<Body> {
    // ...
}
```

### Rationale

#### 1. Eliminates Boilerplate

Manual implementation (without macro):
```rust
impl FromRef<AppState> for AppConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

impl FromRef<AppState> for Arc<DashMap<String, TokenBucket>> {
    fn from_ref(state: &AppState) -> Self {
        state.client_buckets.clone()
    }
}

// × 2 impls = 12 lines of boilerplate
```

With macro:
```rust
#[derive(Clone, FromRef)]
struct AppState { ... }
// ✓ One-liner, auto-generates both impls
```

#### 2. Automatic Field Type Mapping

Macro generates correct extraction for each field type:

```rust
#[derive(Clone, FromRef)]
struct AppState {
    config: AppConfig,                                    // → impl FromRef<AppState> for AppConfig
    client_buckets: Arc<DashMap<String, TokenBucket>>,   // → impl FromRef<AppState> for Arc<DashMap<...>>
}

// Handlers use extracted types directly:
async fn handler(State(config): State<AppConfig>) { }
async fn handler(State(buckets): State<Arc<DashMap<String, TokenBucket>>>) { }
```

#### 3. Composability

Scaling to many fields:

```rust
#[derive(Clone, FromRef)]
struct AppState {
    config: AppConfig,
    client_buckets: Arc<DashMap<String, TokenBucket>>,
    metrics: Arc<Metrics>,
    cache: Arc<Cache>,
}
// ✓ Macro generates 4 FromRef impls automatically
```

Without macro, would need 4 manual impl blocks.

#### 4. Maintainability

Adding a new field:
```rust
#[derive(Clone, FromRef)]
struct AppState {
    config: AppConfig,
    client_buckets: Arc<DashMap<String, TokenBucket>>,
    logger: Arc<Logger>,  // ← New field
}
// ✓ Handlers can now use State(logger): State<Arc<Logger>>
// ✓ No manual impl needed
```

### Trade-offs & Alternatives

#### 1. Manual impl FromRef
```rust
impl FromRef<AppState> for AppConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}
```

**Pros:** Explicit, no macro magic  
**Cons:**
- Boilerplate (one impl per field)
- Duplication (field name repeated 3x)
- Error-prone (typos in impl)

**Verdict:** Unnecessary boilerplate for simple delegation.

#### 2. Pass Entire AppState
```rust
async fn rate_limit_handler(
    State(state): State<AppState>,
) -> Response<Body> {
    let config = &state.config;
    let buckets = &state.client_buckets;
    // ...
}
```

**Pros:** No extraction, no macro  
**Cons:**
- Handlers coupled to AppState structure
- Harder to test (need full state)
- Less ergonomic (extra dereference)

**Verdict:** Couples handlers to state structure; FromRef better isolates concerns.

#### 3. Tower Layers
```rust
// Separate state per middleware layer
.layer(ConfigLayer::new(config))
.layer(BucketLayer::new(buckets))
```

**Pros:** Fine-grained layer control  
**Cons:**
- Overkill for simple state passing
- More complex setup
- Layers intended for cross-cutting concerns (logging, auth)

**Verdict:** Over-engineered for app-specific state.

#### 4. Custom Derive (DIY)
```rust
#[derive(CustomFromRef)]  // ← Custom macro
struct AppState { ... }
```

**Pros:** Full control  
**Cons:**
- Reinvents the wheel
- Maintains custom macro code
- Axum macro already battle-tested

**Verdict:** Don't reinvent; use axum's solution.

## Implementation

### Dependency

```toml
[dependencies]
axum-macros = "0.5"
```

### State Structure

```rust
use axum::extract::FromRef;

#[derive(Clone, FromRef)]
pub struct AppState {
    config: AppConfig,
    client_buckets: Arc<DashMap<String, TokenBucket>>,
}
```

**Requirements:**
- AppState must implement Clone
- Each field type must be cloneable
- Struct derives FromRef (macro generates impls)

### Handler Extraction

```rust
async fn rate_limit_handler(
    State(client_buckets): State<Arc<DashMap<String, TokenBucket>>>,
    State(config): State<AppConfig>,
    headers: HeaderMap,
) -> Response<Body> {
    // config and client_buckets extracted from AppState
    // Axum calls: AppConfig::from_ref(&state)
    //             Arc<DashMap<...>>::from_ref(&state)
}
```

### Startup

```rust
#[tokio::main]
async fn main() {
    let app_config = envy::from_env::<AppConfig>()?;
    let client_buckets = Arc::new(Mutex::new(vec![]));
    
    let app_state = AppState {
        config: app_config,
        client_buckets,
    };
    
    let router = create_routes()
        .with_state(app_state);
    
    // ...
}
```

## Validation

✓ `#[derive(Clone, FromRef)]` compiles  
✓ Handlers extract individual fields  
✓ State cloning works across requests  
✓ Types are inferred correctly  

## Future Enhancements

- [ ] Add custom extractors for complex fields
- [ ] Implement tracing/logging extractors
- [ ] Type-safe state validation layer

## References

- [axum-macros: FromRef derive](https://docs.rs/axum-macros/latest/axum_macros/derive.FromRef.html)
- [Axum: State extraction](https://docs.rs/axum/latest/axum/extract/struct.State.html)
- [Tower: State management](https://docs.rs/tower/latest/tower/layer/trait.Layer.html)
