# Quality Guidelines

`aether-http` trades breadth for stability. The crate should stay small,
serializable, easy to test, and usable from both long-running gateway services
and the standalone proxy binary.

## Public API Shape

Expose shared helpers through `src/lib.rs` and keep modules private:

```rust
// crates/aether-http/src/lib.rs:5
pub use client::{apply_http_client_config, build_http_client, build_http_client_with_headers};
pub use config::{HttpClientConfig, HttpRetryConfig};
pub use retry::jittered_delay_for_retry;
```

New public items must be broadly useful and should be imported as
`aether_http::SomeType` or `aether_http::some_helper`. Avoid nested module API
contracts unless the crate grows enough to require them.

## Type Safety

Use plain, serializable config structs for settings that cross config files,
tests, or process boundaries:

```rust
// crates/aether-http/src/config.rs:1
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct HttpClientConfig {
```

Use `Option<u64>` for optional millisecond values, and use `usize` only where
the underlying reqwest API expects it:

```rust
// crates/aether-http/src/config.rs:3
pub connect_timeout_ms: Option<u64>,
pub request_timeout_ms: Option<u64>,
pub pool_idle_timeout_ms: Option<u64>,
pub pool_max_idle_per_host: Option<usize>,
```

Use `Copy` only for small value types whose semantics are value-like. The retry
config is a good example:

```rust
// crates/aether-http/src/config.rs:32
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct HttpRetryConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}
```

## Defaults

Defaults should be conservative and production-safe. The HTTP client defaults
to Rustls TLS, TCP_NODELAY, no timeout overrides, no proxy, and no default user
agent:

```rust
// crates/aether-http/src/config.rs:15
impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout_ms: None,
            request_timeout_ms: None,
            pool_idle_timeout_ms: None,
            pool_max_idle_per_host: None,
            tcp_keepalive_ms: None,
            tcp_nodelay: true,
            http2_adaptive_window: false,
            use_rustls_tls: true,
            user_agent: None,
            proxy_url: None,
        }
    }
}
```

Do not add a default timeout just because a single caller wants one. Callers set
their own operational budget, for example the gateway state client uses a
five-minute request timeout:

```rust
// apps/aether-gateway/src/state/core.rs:177
let client = build_http_client(&HttpClientConfig {
    connect_timeout_ms: Some(10_000),
    request_timeout_ms: Some(300_000),
    http2_adaptive_window: true,
    ..HttpClientConfig::default()
})?;
```

## Builder Composition

`apply_http_client_config` should remain composable: callers can set redirect,
HTTP version, certificates, headers, or transport profile before and after the
shared config is applied.

```rust
// apps/aether-gateway/src/execution_runtime/transport.rs:871
fn build_client(
    timeouts: Option<&aether_contracts::ExecutionTimeouts>,
    proxy: Option<&ProxySnapshot>,
    transport_profile: Option<&ResolvedTransportProfile>,
    transport_controls: ExecutionTransportControls,
) -> Result<reqwest::Client, ExecutionRuntimeTransportError> {
```

```rust
// apps/aether-gateway/src/execution_runtime/transport.rs:878
let mut builder = reqwest::Client::builder();
if transport_controls.follow_redirects != Some(true) {
    builder = builder.redirect(Policy::none());
}
```

The shared helper then applies generic config:

```rust
// apps/aether-gateway/src/execution_runtime/transport.rs:885
let mut builder = apply_http_client_config(
    builder,
    &HttpClientConfig {
        connect_timeout_ms: timeouts.and_then(|timeouts| timeouts.connect_ms),
        ..HttpClientConfig::default()
    },
);
```

Guideline: prefer builder composition over adding a new `HttpClientConfig` field
for every caller-specific `reqwest::ClientBuilder` option.

## Retry Quality

Normalize retry policy at creation time when a caller stores it:

```rust
// apps/aether-proxy/src/registration/client.rs:75
let retry = HttpRetryConfig {
    max_attempts: config.aether_retry_max_attempts,
    base_delay_ms: config.aether_retry_base_delay_ms,
    max_delay_ms: config.aether_retry_max_delay_ms,
}
.normalized();
```

Long-running retry loops can use an effectively unlimited attempt count, but
they should still normalize the backoff parameters:

```rust
// apps/aether-proxy/src/app.rs:439
fn registration_retry_policy(config: &Config) -> HttpRetryConfig {
    HttpRetryConfig {
        max_attempts: u32::MAX,
        base_delay_ms: config.aether_retry_base_delay_ms,
        max_delay_ms: config.aether_retry_max_delay_ms,
    }
    .normalized()
}
```

## Testing Pattern

Tests are local to each helper module and target behavior that can regress
without network I/O.

Client construction with headers is tested in `client.rs`:

```rust
// crates/aether-http/src/client.rs:69
#[test]
fn builds_client_with_default_headers() {
    let mut headers = HeaderMap::new();
    headers.insert("x-test", HeaderValue::from_static("ok"));
```

Retry normalization and caps are tested in `config.rs`:

```rust
// crates/aether-http/src/config.rs:76
#[test]
fn normalizes_retry_bounds() {
    let config = HttpRetryConfig {
        max_attempts: 0,
        base_delay_ms: 0,
        max_delay_ms: 5,
    }
    .normalized();
```

Jitter has a minimal invariant test because exact timing is intentionally
non-deterministic:

```rust
// crates/aether-http/src/retry.rs:24
#[test]
fn jittered_delay_is_at_least_base_delay() {
```

When adding a helper, add unit tests in the same source file. Do not introduce
network calls, timers that actually sleep, or service fixtures into this crate's
unit tests.

## Forbidden Patterns

DON'T add dependencies on application crates:

```toml
# DON'T: crates/aether-http/Cargo.toml
aether-gateway = { path = "../../apps/aether-gateway" }
aether-data = { path = "../aether-data" }
```

The current dependency list is intentionally only:

```toml
# crates/aether-http/Cargo.toml:8
[dependencies]
reqwest.workspace = true
serde.workspace = true
```

DON'T put domain retry decisions in the foundation crate:

```rust
// DON'T: belongs in the caller that knows the operation.
pub fn should_retry_registration_status(status: StatusCode) -> bool { ... }
```

The actual proxy-specific status policy is local to the proxy registration
client:

```rust
// apps/aether-proxy/src/registration/client.rs:251
fn should_retry_status(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
}
```

DON'T replace saturating retry math with unchecked exponentiation or
multiplication:

```rust
// DON'T: can overflow on large retry_index.
let delay_ms = config.base_delay_ms * 2_u64.pow(retry_index);
```

Use the existing bounded implementation instead:

```rust
// crates/aether-http/src/config.rs:63
let factor = 2_u64.saturating_pow(retry_index.min(20));
let delay_ms = config
    .base_delay_ms
    .saturating_mul(factor)
    .min(config.max_delay_ms);
```

## Review Checklist

- Is the API useful outside one caller?
- Does the crate still have no internal Aether dependencies?
- Are config units encoded in field names?
- Does any new fallible behavior return the narrowest useful error type?
- Are invalid optional string configs trimmed and handled intentionally?
- Are tests deterministic and local?
- Did `src/lib.rs` remain the public facade?
- Did callers keep domain policy outside `aether-http`?

