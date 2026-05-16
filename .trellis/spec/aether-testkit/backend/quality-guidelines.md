# Quality Guidelines

`aether-testkit` should make integration tests and baseline experiments
deterministic, easy to tear down, and close to production behavior. Prefer
small wrappers around real Aether routers and runtime/data components over
hand-rolled fakes.

## Public API Discipline

Keep modules private and re-export only stable helpers through `src/lib.rs`.

```rust
// crates/aether-testkit/src/lib.rs:22
pub use metrics::{
    fetch_prometheus_samples, find_metric_value_u64, parse_prometheus_samples, PrometheusSample,
};
pub use postgres::{prepare_aether_postgres_schema, ManagedPostgresServer};
pub use redis::ManagedRedisServer;
```

Guideline: new public API must be useful to integration tests or baseline
binaries outside its module. Scenario-only helpers stay private in `src/bin/*`.

## Visibility Pattern

Configuration structs expose fields so tests can construct exact scenarios.
Runtime owner structs keep internals private and expose only read-only accessors.

```rust
// crates/aether-testkit/src/gateway.rs:6
#[derive(Debug, Clone)]
pub struct GatewayHarnessConfig {
    pub upstream_base_url: String,
    pub data_config: Option<GatewayDataConfig>,
    pub max_in_flight_requests: Option<usize>,
    pub distributed_request_gate: Option<RuntimeSemaphore>,
    pub tunnel_instance_id: Option<String>,
    pub tunnel_relay_base_url: Option<String>,
}
```

```rust
// crates/aether-testkit/src/gateway.rs:29
#[derive(Debug)]
pub struct GatewayHarness {
    server: SpawnedServer,
}
```

DON'T expose `SpawnedServer.handle`, child process handles, temp directories, or
mutable internal state. Public callers should not be able to orphan processes or
abort background tasks manually.

## Deterministic Output

Use `BTreeMap` for report and metrics data that is serialized or compared in
tests.

```rust
// crates/aether-testkit/src/load.rs:78
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct MultiUrlHttpLoadProbeResult {
    pub target_urls: Vec<String>,
    pub target_request_counts: BTreeMap<String, usize>,
    pub method: String,
    pub response_mode: HttpLoadProbeResponseMode,
    pub total_requests: usize,
```

```rust
// crates/aether-testkit/src/metrics.rs:86
fn parse_labels(raw: &str) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
```

Guideline: if a value is printed as JSON or asserted in tests, prefer
deterministic ordering. Use `HashMap` only when protocol types or production
APIs require it.

## Type Safety

Prefer explicit config/result structs over tuple returns from helpers with more
than one field.

```rust
// crates/aether-testkit/src/load.rs:62
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct HttpLoadProbeResult {
    pub url: String,
    pub method: String,
    pub response_mode: HttpLoadProbeResponseMode,
    pub total_requests: usize,
    pub concurrency: usize,
```

Enums should derive the traits needed for JSON reports and assertions.

```rust
// crates/aether-testkit/src/load.rs:10
#[derive(Debug, Clone, Copy, Default, serde::Serialize, PartialEq, Eq)]
pub enum HttpLoadProbeResponseMode {
    #[default]
    HeadersOnly,
    FullBody,
}
```

Guideline: avoid boolean mode arguments in new public APIs when the mode is
reused across several functions or serialized into reports. Use a small enum.

## Real Production Surfaces

Harnesses must build actual Aether routers with production state builders.

```rust
// crates/aether-testkit/src/gateway.rs:47
let mut state = AppState::new()
    .map_err(|err| format!("failed to build gateway harness state: {err}"))?;
```

```rust
// crates/aether-testkit/src/tunnel.rs:50
let state = TunnelRuntimeState::new(
    TunnelControlPlaneClient::disabled(),
    TunnelConnConfig {
        ping_interval: config.ping_interval,
        idle_timeout: Duration::from_secs(0),
        outbound_queue_capacity: config.outbound_queue_capacity,
    },
    config.max_streams,
)
```

DON'T reimplement gateway, tunnel, or execution runtime behavior in testkit
fakes. If a scenario needs a new production knob, expose it through the harness
config and pass it into the real builder.

## Concurrency Style

For fixed-size request pressure, use atomics for counters and `JoinSet` for
worker lifetimes.

```rust
// crates/aether-testkit/src/load.rs:145
let next_request = Arc::new(AtomicUsize::new(0));
let latencies_ms = Arc::new(Mutex::new(Vec::with_capacity(config.total_requests)));
let status_counts = Arc::new(Mutex::new(BTreeMap::<u16, usize>::new()));
let target_request_counts = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
let failed_requests = Arc::new(AtomicUsize::new(0));
let completed_requests = Arc::new(AtomicUsize::new(0));
```

```rust
// crates/aether-testkit/src/load.rs:152
let mut workers = tokio::task::JoinSet::new();
for _ in 0..config.concurrency {
```

Guideline: counters can be atomic; collections that need snapshots should use a
small `tokio::sync::Mutex`. Always drain `JoinSet` and propagate join errors.

## Lifecycle Cleanup

All local server/process owners must clean themselves up on drop.

```rust
// crates/aether-testkit/src/server.rs:53
impl Drop for SpawnedServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
```

```rust
// crates/aether-testkit/src/redis.rs:103
impl Drop for ManagedRedisServer {
    fn drop(&mut self) {
        let _ = self.stop();
        let _ = std::fs::remove_dir_all(&self.workdir);
    }
}
```

Guideline: test helpers are allowed to ignore cleanup errors in `Drop`, but
startup/restart paths must return errors. Do not require callers to remember a
manual cleanup method for the common path.

## Local Ports

Bind only loopback addresses and use ephemeral ports unless a scenario
explicitly needs a fixed port restart.

```rust
// crates/aether-testkit/src/server.rs:59
pub fn reserve_local_port() -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
```

Fixed-port restart scenarios should reserve a port first, then retry startup on
the same port.

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:447
let port = reserve_local_port()?;
let tunnel_config = TunnelHarnessConfig::default();
let initial_tunnel = TunnelHarness::start_on_port(tunnel_config.clone(), port).await?;
```

DON'T bind `0.0.0.0` or hardcode shared ports in tests. That creates flaky CI
behavior and can expose local-only test services.

## Tests

Keep unit tests next to parser, validation, and polling helpers.

```rust
// crates/aether-testkit/src/load.rs:284
#[test]
fn validates_probe_config() {
    assert!(HttpLoadProbeConfig {
        url: String::new(),
        ..HttpLoadProbeConfig::default()
    }
    .validate()
    .is_err());
```

```rust
// crates/aether-testkit/src/wait.rs:33
#[tokio::test]
async fn returns_true_when_predicate_eventually_passes() {
    let flag = Arc::new(AtomicBool::new(false));
```

Guideline: add tests when changing pure parsing, validation, latency math, or
polling behavior. Process-level helpers can be covered by integration/baseline
tests because they require local binaries like `postgres` and `redis-server`.

## Serialization and Reports

Baseline reports should derive `Serialize` and be printed as pretty JSON.

```rust
// crates/aether-testkit/src/bin/capacity_curve_baseline.rs:56
#[derive(Debug, Serialize)]
struct CapacityCurveBaselineReport {
    suite: &'static str,
    gateway_sync: CapacityCurveScenarioReport,
```

```rust
// crates/aether-testkit/src/bin/capacity_curve_baseline.rs:115
let report = run_suite(&config).await?;
let raw = serde_json::to_string_pretty(&report)?;
println!("{raw}");
```

Guideline: stdout is machine-readable JSON for report binaries. Human usage
text belongs on stderr.

## Dependency Boundaries

`aether-testkit` is allowed to depend on application/runtime/data crates because
it sits at the application layer.

```toml
# crates/aether-testkit/Cargo.toml:11
aether-data.workspace = true
aether-contracts.workspace = true
aether-gateway.workspace = true
aether-http.workspace = true
aether-runtime.workspace = true
aether-runtime-state.workspace = true
```

Do not add new external dependencies for convenience. Reuse workspace crates and
standard library types unless the testkit has a concrete gap.

## DON'T

```rust
// DON'T: shared mutable Vec without bounded worker lifecycle.
let mut handles = Vec::new();
for request in requests {
    handles.push(tokio::spawn(async move { send(request).await }));
}
```

Prefer the crate pattern: `JoinSet`, an atomic next index, and explicit join
error propagation.

Do not add sleeps where readiness polling is needed. Use `wait_until` with a
timeout and a predicate that proves the dependency is available.

Do not make fixtures globally unique with wall-clock timestamps unless the
caller needs uniqueness. `test_trace_id(prefix)` intentionally returns a stable
string for deterministic assertions:

```rust
// crates/aether-testkit/src/fixtures.rs:1
pub fn test_trace_id(prefix: &str) -> String {
    format!("{prefix}-test-trace")
}
```
