# Logging Guidelines

`aether-testkit` does not use direct `tracing::debug!`, `info!`, `warn!`, or
`error!` calls today. Observability is initialized through `aether-runtime`,
and baseline programs emit their primary results as JSON. Preserve that split:
runtime telemetry goes through the shared runtime, while report data goes to
stdout.

## Runtime Initialization

All binaries that exercise production runtime components should initialize the
test runtime before parsing arguments or starting services.

```rust
// crates/aether-testkit/src/tracing.rs:7
pub fn test_runtime_config(service_name: &'static str) -> ServiceRuntimeConfig {
    ServiceRuntimeConfig::new(service_name, "aether_testkit=debug")
        .with_metrics_namespace("aether_testkit")
}
```

```rust
// crates/aether-testkit/src/tracing.rs:12
pub fn init_test_runtime_for(service_name: &'static str) {
    let _ = init_service_runtime(test_runtime_config(service_name));
}
```

The return value is intentionally ignored so repeated setup in tests does not
fail when the global runtime subscriber has already been initialized.

## Service Names

Use stable suite-specific service names. This makes traces and metrics
distinguishable across baseline binaries.

```rust
// crates/aether-testkit/src/bin/capacity_curve_baseline.rs:111
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_test_runtime_for("capacity-curve-baseline");
```

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:166
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_test_runtime_for("failure-recovery-baseline");
```

Guideline: new binaries should call `init_test_runtime_for("<binary-name>")`
where the name matches the file stem with underscores converted to hyphens.

## Log Levels

The crate-level filter is currently `"aether_testkit=debug"`. Do not add
per-request `debug!` calls inside hot load loops unless the logs are behind a
new, intentionally narrow target. Load tests can produce hundreds or thousands
of requests and logging each one changes the thing being measured.

Use these rules if direct tracing is added later:

- `debug`: lifecycle details that help diagnose flaky harness setup.
- `info`: one-time suite phase transitions, never every request.
- `warn`: degraded test environment where the suite can continue.
- `error`: setup failure immediately before returning an error.

## Structured Fields

If new tracing calls are needed, include fields that match the crate's domain:

- `service_name`
- `port`
- `base_url`
- `suite`
- `scenario`
- `gate`
- `total_requests`
- `concurrency`
- `failed_requests`

Do not log full request bodies, API keys, Redis URLs with credentials, database
URLs with credentials, WebSocket relay envelopes, or provider headers.

## Report Output Is Not Logging

Baseline binaries print exactly one pretty JSON report to stdout, then
optionally write the same report to a file.

```rust
// crates/aether-testkit/src/bin/capacity_curve_baseline.rs:115
let report = run_suite(&config).await?;
let raw = serde_json::to_string_pretty(&report)?;
println!("{raw}");
if let Some(path) = config.output_path.as_ref() {
```

Usage text goes to stderr, not stdout, so scripts can parse stdout as JSON.

```rust
// crates/aether-testkit/src/bin/http_load_probe.rs:88
fn print_usage() {
    eprintln!(
        "usage: cargo run -p aether-testkit --bin http_load_probe -- --url URL [--method METHOD] [--header NAME=VALUE] [--body BODY] [--total-requests N] [--concurrency N] [--timeout-ms N] [--full-body]"
    );
}
```

DON'T add progress `println!` calls to stdout in report binaries. If humans
need progress, use stderr or tracing and verify JSON consumers are unaffected.

## Child Process Output

Postgres redirects stdout/stderr to a log file and includes that file in timeout
errors.

```rust
// crates/aether-testkit/src/postgres.rs:88
let log_path = self.workdir.join("postgres.log");
let stdout = std::fs::File::create(&log_path)?;
let stderr = stdout.try_clone()?;
let child = Command::new(&self.postgres_bin)
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

Redis output is intentionally suppressed because readiness is tested with a
protocol-level PING.

```rust
// crates/aether-testkit/src/redis.rs:60
let child = Command::new(&self.binary)
    .arg("--save")
    .arg("")
    .arg("--appendonly")
    .arg("no")
```

```rust
// crates/aether-testkit/src/redis.rs:71
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()?;
```

Guideline: capture child logs when they materially help diagnose startup
failures. Suppress them when a protocol readiness check gives enough signal.

## Metrics

Metrics are a first-class observability surface in this crate. The runtime
namespace is set to `aether_testkit`, and baseline programs fetch Prometheus
metrics from harness endpoints.

```rust
// crates/aether-testkit/src/bin/capacity_curve_baseline.rs:422
async fn capture_gate_metrics(
    metrics_url: &str,
    gate_name: &str,
) -> Result<GateMetricSnapshot, Box<dyn std::error::Error>> {
    let samples = fetch_prometheus_samples(metrics_url)
        .await
        .map_err(std::io::Error::other)?;
```

```rust
// crates/aether-testkit/src/metrics.rs:37
pub fn find_metric_value_u64(
    samples: &[PrometheusSample],
    metric_name: &str,
    labels: &[(&str, &str)],
) -> Option<u64> {
```

Guideline: prefer metrics snapshots in JSON reports over scraping log text.

## Sensitive Data

The testkit constructs URLs, headers, JSON request bodies, and local database
URLs. Treat all of them as potentially sensitive unless they are known synthetic
fixtures.

Safe to show in errors/logs:

- loopback ports;
- synthetic suite names;
- Postgres startup logs from the local temp cluster;
- aggregate counters and latency values.

Do not show:

- user-provided request headers from load probes;
- request bodies passed through `HttpLoadProbeConfig.body`;
- full Redis/Postgres URLs if a future caller supplies credentials;
- tunnel binary frame payloads.

## DON'T

```rust
// DON'T: changes stdout contract for report binaries.
println!("starting request {current}");
```

```rust
// DON'T: noisy and can leak headers/body data.
debug!(?request, ?headers, ?body, "load probe request");
```

Prefer one-time suite tracing through `init_test_runtime_for` plus structured
JSON result fields.
