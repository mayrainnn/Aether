# Error Handling

`aether-http` does not define a custom error enum. Its helpers either return a
configured `reqwest::ClientBuilder`, return `Result<reqwest::Client,
reqwest::Error>`, or return `std::time::Duration`. Domain-specific error
mapping belongs to callers.

## Error Surface

`apply_http_client_config` cannot fail because it only applies builder settings
that do not validate external input:

```rust
// crates/aether-http/src/client.rs:7
pub fn apply_http_client_config(
    mut builder: reqwest::ClientBuilder,
    config: &HttpClientConfig,
) -> reqwest::ClientBuilder {
```

`build_http_client` and `build_http_client_with_headers` can fail because
`reqwest` validates proxy URLs and because `builder.build()` can return
`reqwest::Error`:

```rust
// crates/aether-http/src/client.rs:39
pub fn build_http_client(config: &HttpClientConfig) -> Result<reqwest::Client, reqwest::Error> {
    build_http_client_with_headers(config, HeaderMap::new())
}
```

```rust
// crates/aether-http/src/client.rs:43
pub fn build_http_client_with_headers(
    config: &HttpClientConfig,
    default_headers: HeaderMap,
) -> Result<reqwest::Client, reqwest::Error> {
```

Keep this error type unchanged unless the crate starts to own multiple
independent failure categories. A custom wrapper today would only erase useful
`reqwest` context.

## Proxy Validation

Proxy URL validation happens at the boundary where `HttpClientConfig.proxy_url`
is converted into a `reqwest::Proxy`. Empty strings are deliberately ignored
after trimming:

```rust
// crates/aether-http/src/client.rs:48
if let Some(proxy_url) = config
    .proxy_url
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty())
{
    builder = builder.proxy(reqwest::Proxy::all(proxy_url)?);
}
```

Guideline: preserve the trim-and-ignore-empty behavior. It lets environment or
TOML plumbing pass an empty optional proxy without turning startup into an
error.

DON'T silently swallow invalid non-empty proxy URLs:

```rust
// DON'T: invalid proxies must still fail client construction.
if let Some(proxy_url) = &config.proxy_url {
    if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
        builder = builder.proxy(proxy);
    }
}
```

The current `?` is important because callers like proxy registration treat
client construction as startup-critical.

## Caller Error Mapping

Callers map `reqwest::Error` into their own error type when they have one. The
gateway transport layer wraps client build errors in `ExecutionRuntimeTransportError`:

```rust
// apps/aether-gateway/src/execution_runtime/transport.rs:784
builder
    .build()
    .map_err(ExecutionRuntimeTransportError::ClientBuild)
```

The gateway app state build path leaves the `reqwest::Error` in the local
constructor return type:

```rust
// apps/aether-gateway/src/state/core.rs:171
fn build(execution_runtime_override_base_url: Option<String>) -> Result<Self, reqwest::Error> {
```

The proxy registration client makes HTTP client creation a process startup
invariant:

```rust
// apps/aether-proxy/src/registration/client.rs:57
let http = build_http_client(&HttpClientConfig {
    connect_timeout_ms: Some(config.aether_connect_timeout_secs.saturating_mul(1_000)),
```

```rust
// apps/aether-proxy/src/registration/client.rs:72
})
.expect("failed to create HTTP client");
```

Do not move those caller choices into `aether-http`. This crate should expose
the precise construction failure and let the application decide whether to
degrade, bail, or panic.

## Retry Error Rules

`HttpRetryConfig` protects retry math from invalid zero values. It normalizes
attempts and delay bounds before a delay is computed:

```rust
// crates/aether-http/src/config.rs:49
impl HttpRetryConfig {
    pub fn normalized(self) -> Self {
        let max_attempts = self.max_attempts.max(1);
        let base_delay_ms = self.base_delay_ms.max(1);
        let max_delay_ms = self.max_delay_ms.max(base_delay_ms);
```

`delay_for_retry` uses saturating math and caps exponential growth:

```rust
// crates/aether-http/src/config.rs:61
pub fn delay_for_retry(self, retry_index: u32) -> std::time::Duration {
    let config = self.normalized();
    let factor = 2_u64.saturating_pow(retry_index.min(20));
    let delay_ms = config
        .base_delay_ms
        .saturating_mul(factor)
        .min(config.max_delay_ms);
```

Guideline: retry timing helpers should return a `Duration`, not a `Result`.
Invalid retry inputs are normalized because retry loops are defensive
availability code.

## Time Source Failure

`jittered_delay_for_retry` handles `SystemTime` anomalies by falling back to
zero jitter:

```rust
// crates/aether-http/src/retry.rs:11
let nanos = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| duration.subsec_nanos() as u64)
    .unwrap_or(0);
```

Do not convert this into a hard failure. Retry delay calculation must not break
the retry path because the local clock produced an unexpected result.

## Common Mistakes

- DON'T add `anyhow` or `thiserror` to `aether-http` just to wrap
  `reqwest::Error`; callers already decide their error shape.
- DON'T log errors in this crate. There is no request label, user, route, or
  domain operation here.
- DON'T treat `HttpRetryConfig::max_attempts` as a loop counter inside this
  crate. The retry loop belongs to callers like
  `apps/aether-proxy/src/registration/client.rs:202`.
- DON'T return `Option<reqwest::Client>` from builder helpers. A failed client
  build carries actionable error detail.
- DON'T accept invalid non-empty proxy URLs. The current `?` is the fail-fast
  contract.

