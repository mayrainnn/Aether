# Directory Structure

`crates/aether-http/` is a compact foundation crate. It should remain a leaf
utility package for shared outbound HTTP client configuration and retry delay
calculation. New files are rare and should only appear when a pattern is shared
by multiple Aether binaries or service crates.

## Actual Layout

```text
crates/aether-http/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── client.rs
    ├── config.rs
    └── retry.rs
```

ABCoder MCP confirmed this same structure: module `aether-http`, packages
`aether-http`, `aether-http::client`, `aether-http::config`, and
`aether-http::retry`, with one source file in each package.

## Module Responsibilities

`src/lib.rs` is the public facade. It declares private modules and re-exports
only the stable public surface:

```rust
// crates/aether-http/src/lib.rs:1
mod client;
mod config;
mod retry;

pub use client::{apply_http_client_config, build_http_client, build_http_client_with_headers};
pub use config::{HttpClientConfig, HttpRetryConfig};
pub use retry::jittered_delay_for_retry;
```

Do not make `client`, `config`, or `retry` public modules unless callers need
module namespaces. Current consumers import from `aether_http::{...}`.

`src/config.rs` owns serializable configuration data. It contains plain data
types, defaults, normalization, and deterministic retry delay math:

```rust
// crates/aether-http/src/config.rs:1
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct HttpClientConfig {
    pub connect_timeout_ms: Option<u64>,
    pub request_timeout_ms: Option<u64>,
    pub pool_idle_timeout_ms: Option<u64>,
    pub pool_max_idle_per_host: Option<usize>,
    pub tcp_keepalive_ms: Option<u64>,
    pub tcp_nodelay: bool,
    pub http2_adaptive_window: bool,
    pub use_rustls_tls: bool,
    pub user_agent: Option<String>,
    pub proxy_url: Option<String>,
}
```

`src/client.rs` owns `reqwest::ClientBuilder` wiring. Keep builder-only behavior
here so callers can apply shared defaults before adding caller-specific settings:

```rust
// crates/aether-http/src/client.rs:7
pub fn apply_http_client_config(
    mut builder: reqwest::ClientBuilder,
    config: &HttpClientConfig,
) -> reqwest::ClientBuilder {
```

`src/retry.rs` owns non-deterministic jitter. It builds on
`HttpRetryConfig::delay_for_retry` and adds a small time-derived offset:

```rust
// crates/aether-http/src/retry.rs:5
pub fn jittered_delay_for_retry(config: HttpRetryConfig, retry_index: u32) -> Duration {
    let base = config.delay_for_retry(retry_index);
```

## Dependency Boundary

`crates/aether-http/Cargo.toml:8` lists only `reqwest` and `serde`. That is the
intentional boundary. This package must not learn about:

- Axum request handlers
- SeaORM entities or transactions
- Redis or cache layers
- Gateway state
- Proxy registration models
- Scheduler or provider domain models
- Runtime metrics sinks

Use the crate to express generic HTTP client setup. Put domain-specific
behavior in callers.

## Consumer Pattern

Callers customize the shared config and keep the domain decision local. The
gateway relay client uses shared builder application but maps build errors into
its own domain error:

```rust
// apps/aether-gateway/src/execution_runtime/transport.rs:773
fn build_relay_client(
    timeouts: Option<&aether_contracts::ExecutionTimeouts>,
) -> Result<reqwest::Client, ExecutionRuntimeTransportError> {
    let builder = apply_http_client_config(
        reqwest::Client::builder(),
        &HttpClientConfig {
            connect_timeout_ms: timeouts.and_then(|timeouts| timeouts.connect_ms),
            use_rustls_tls: false,
            ..HttpClientConfig::default()
        },
    );
```

The proxy registration client uses the same config type for many knobs from
runtime configuration:

```rust
// apps/aether-proxy/src/registration/client.rs:57
let http = build_http_client(&HttpClientConfig {
    connect_timeout_ms: Some(config.aether_connect_timeout_secs.saturating_mul(1_000)),
    request_timeout_ms: Some(config.aether_request_timeout_secs.saturating_mul(1_000)),
    pool_idle_timeout_ms: Some(config.aether_pool_idle_timeout_secs.saturating_mul(1_000)),
    pool_max_idle_per_host: Some(config.aether_pool_max_idle_per_host),
```

## Adding New Files

Add a new file only when a separate shared concept emerges. Good candidates:

- `headers.rs` if multiple crates need the same safe default header handling.
- `proxy.rs` if proxy URL validation expands beyond `reqwest::Proxy::all`.
- `timeout.rs` if timeout conversion becomes more than direct milliseconds.

Do not add `service.rs`, `state.rs`, `handlers.rs`, `database.rs`, or
`metrics.rs` under this crate. Those names imply responsibilities outside the
foundation HTTP utility scope.

## Naming Conventions

Types use the crate prefix plus the concept: `HttpClientConfig` and
`HttpRetryConfig`. Functions are action-oriented and explicit:
`apply_http_client_config`, `build_http_client`, `build_http_client_with_headers`,
and `jittered_delay_for_retry`.

Use millisecond suffixes on config fields that represent durations:
`connect_timeout_ms`, `request_timeout_ms`, `base_delay_ms`, and
`max_delay_ms`. This avoids unit ambiguity across TOML, JSON, tests, and CLI
configuration.

## DON'T Patterns

DON'T expose module internals just to make imports shorter:

```rust
// DON'T: crates/aether-http/src/lib.rs
pub mod client;
pub mod config;
pub mod retry;
```

Prefer the existing facade because all current callers import from
`aether_http::{build_http_client, HttpClientConfig}` or similar.

DON'T place caller-specific policy in this crate:

```rust
// DON'T: aether-http must not know gateway status codes or proxy node labels.
pub fn retry_proxy_registration_status(status: reqwest::StatusCode) -> bool { ... }
```

Keep that decision where it already lives, for example
`apps/aether-proxy/src/registration/client.rs:251`.
