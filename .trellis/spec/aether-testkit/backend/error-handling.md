# Error Handling

`aether-testkit` favors simple, caller-friendly errors over a crate-wide error
enum. The API boundary determines the error type:

- low-level process/server helpers return `std::io::Error` when the failure is
  primarily IO or socket related;
- harness helpers return `Result<_, String>` when they adapt several production
  builders into a test-only startup API;
- baseline binaries return `Result<_, Box<dyn std::error::Error>>` so CLI
  orchestration can use `?` across IO, parse, WebSocket, database, and harness
  failures;
- parser/probe helpers return `Result<_, String>` for concise assertion output.

## No Crate-Wide Error Enum

There is no `AetherTestkitError` today. Do not introduce one unless multiple
public functions need typed matching by callers. Most callers just need a
human-readable failure while setting up a test harness.

```rust
// crates/aether-testkit/src/gateway.rs:35
pub async fn start(config: GatewayHarnessConfig) -> Result<Self, String> {
    Self::start_with_server(config, None).await
}
```

```rust
// crates/aether-testkit/src/postgres.rs:21
pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
    let port = reserve_local_port()?;
```

Guideline: choose the smallest error type that callers can act on. For new
library helpers, prefer `std::io::Error` for pure IO, `String` for test-facing
validation/context, and `Box<dyn Error>` only when orchestration crosses many
error families.

## Context When Adapting Production Builders

Harnesses convert production builder errors into contextual strings before
returning to tests.

```rust
// crates/aether-testkit/src/gateway.rs:47
let mut state = AppState::new()
    .map_err(|err| format!("failed to build gateway harness state: {err}"))?;
if let Some(data_config) = config.data_config {
    state = state
        .with_data_config(data_config)
        .map_err(|err| format!("failed to configure gateway harness data state: {err}"))?;
}
```

```rust
// crates/aether-testkit/src/execution_runtime.rs:37
let server = match port {
    Some(port) => SpawnedServer::start_on_port(port, router)
        .await
        .map_err(|err| format!("failed to start execution runtime harness: {err}"))?,
```

Guideline: when wrapping a production builder, include the harness name and the
failed phase. `"failed to start gateway harness: {err}"` is useful; `"start
failed"` is not.

## `?` Propagation for IO Lifecycles

Local process managers use `?` for the normal IO path and explicitly convert
stateful readiness failures into timeout errors.

```rust
// crates/aether-testkit/src/postgres.rs:40
let init_output = Command::new(&initdb_bin)
    .arg("-D")
    .arg(&data_dir)
    .arg("-U")
    .arg("aether")
    .arg("--auth=trust")
    .arg("--encoding=UTF8")
    .arg("--no-instructions")
    .output()?;
```

```rust
// crates/aether-testkit/src/postgres.rs:125
if !ready {
    self.stop()?;
    let logs = std::fs::read_to_string(&log_path).unwrap_or_default();
    return Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!("timed out waiting for local postgres; logs:\n{logs}"),
    )
    .into());
}
```

Guideline: readiness failures should stop the child process before returning,
and Postgres failures should include the captured log file content.

## Validation Errors Are Explicit Strings

Load-probe validation returns exact messages that tests and CLI users can
understand without looking up the source.

```rust
// crates/aether-testkit/src/load.rs:44
pub fn validate(&self) -> Result<(), String> {
    if self.url.trim().is_empty() {
        return Err("load probe url cannot be empty".to_string());
    }
    if self.total_requests == 0 {
        return Err("load probe total_requests must be positive".to_string());
    }
```

Multi-target probing adds one more precondition:

```rust
// crates/aether-testkit/src/load.rs:121
pub async fn run_multi_url_http_load_probe(
    config: &HttpLoadProbeConfig,
    urls: &[String],
) -> Result<MultiUrlHttpLoadProbeResult, String> {
    config.validate()?;
    if urls.is_empty() {
        return Err("multi-url load probe requires at least one target url".to_string());
    }
```

Guideline: validate before spawning workers or opening sockets. A bad config
should fail immediately and deterministically.

## Header and HTTP Errors

HTTP helper failures keep the affected field in the message.

```rust
// crates/aether-testkit/src/load.rs:242
fn build_headers(headers: &BTreeMap<String, String>) -> Result<HeaderMap, String> {
    let mut result = HeaderMap::new();
    for (name, value) in headers {
        let name = HeaderName::try_from(name.as_str())
            .map_err(|err| format!("invalid load probe header name `{name}`: {err}"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|err| format!("invalid load probe header value for `{name}`: {err}"))?;
```

Metrics fetching preserves the URL and status/body when the endpoint is not
successful.

```rust
// crates/aether-testkit/src/metrics.rs:10
pub async fn fetch_prometheus_samples(url: &str) -> Result<Vec<PrometheusSample>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|err| format!("failed to build metrics http client: {err}"))?;
```

```rust
// crates/aether-testkit/src/metrics.rs:25
if !status.is_success() {
    return Err(format!("metrics endpoint {url} returned {status}: {body}"));
}
```

## Async Worker Errors

Load probes count per-request send/read failures as request failures, but worker
panics or join failures abort the probe.

```rust
// crates/aether-testkit/src/load.rs:182
match request.send().await {
    Ok(response) => {
        let status = response.status().as_u16();
```

```rust
// crates/aether-testkit/src/load.rs:215
while let Some(result) = workers.join_next().await {
    result.map_err(|err| format!("load probe worker task failed: {err}"))?;
}
```

Guideline: expected target failures become result counters; infrastructure
failures in the test harness itself should return an error.

## Binary Orchestration

Baseline binaries use `Box<dyn Error>` at the top level and convert library
`String` errors into IO errors when needed.

```rust
// crates/aether-testkit/src/bin/capacity_curve_baseline.rs:210
let result = run_http_load_probe(&probe)
    .await
    .map_err(std::io::Error::other)?;
```

Argument parsing may return either formatted strings or `std::io::Error` with
`InvalidInput`. Keep messages tied to the exact flag.

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:688
fn next_value(
    iter: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    iter.next()
        .ok_or_else(|| format!("missing value for {flag}").into())
}
```

## Panic and Expect Policy

The crate uses `expect` only for invariants that indicate a broken test harness,
not recoverable external failures.

```rust
// crates/aether-testkit/src/server.rs:21
let handle = tokio::spawn(async move {
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("spawned server should run");
});
```

```rust
// crates/aether-testkit/src/http.rs:16
pub fn test_http_client() -> reqwest::Client {
    build_http_client(&test_http_client_config()).expect("failed to build test HTTP client")
}
```

DON'T use `expect` for CLI input, network responses, database readiness, Redis
readiness, or file output. Those are environmental and should return `Result`.

## DON'T

```rust
// DON'T: loses phase and URL/port context.
some_async_start().await.map_err(|err| err.to_string())?;
```

Prefer:

```rust
// Pattern from crates/aether-testkit/src/tunnel.rs:66
SpawnedServer::start_on_port(port, router)
    .await
    .map_err(|err| format!("failed to start tunnel harness: {err}"))?;
```

Do not swallow readiness failures silently. `wait_until` returns `bool`; callers
must check it and build a meaningful timeout error.
