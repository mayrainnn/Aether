# Directory Structure

`aether-testkit` is an application-layer Rust crate for integration harnesses,
baseline probes, local dependency processes, and test assertions. Keep the crate
organized around reusable test surfaces, not production domain ownership.

## Public Facade

All reusable helpers are private modules re-exported from `src/lib.rs`.

```rust
// crates/aether-testkit/src/lib.rs:1
mod execution_runtime;
mod fixtures;
mod gateway;
mod http;
mod load;
mod metrics;
mod postgres;
mod redis;
mod server;
mod tracing;
mod tunnel;
mod wait;
```

```rust
// crates/aether-testkit/src/lib.rs:14
pub use execution_runtime::{ExecutionRuntimeHarness, ExecutionRuntimeHarnessConfig};
pub use fixtures::test_trace_id;
pub use gateway::{GatewayHarness, GatewayHarnessConfig};
pub use http::{json_body, test_http_client, test_http_client_config};
```

Guideline: add new shared helpers as a private `mod` plus an explicit `pub use`.
Do not expose nested modules as part of the public API unless a downstream crate
needs stable module-level names.

## Source Layout

```text
crates/aether-testkit/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── execution_runtime.rs
    ├── fixtures.rs
    ├── gateway.rs
    ├── http.rs
    ├── load.rs
    ├── metrics.rs
    ├── postgres.rs
    ├── redis.rs
    ├── server.rs
    ├── tracing.rs
    ├── tunnel.rs
    ├── wait.rs
    └── bin/
        ├── capacity_curve_baseline.rs
        ├── dependency_pressure_baseline.rs
        ├── failure_recovery_baseline.rs
        ├── gateway_tunnel_stream_baseline.rs
        ├── http_load_probe.rs
        ├── multi_instance_admission_baseline.rs
        ├── multi_instance_owner_relay_baseline.rs
        ├── redis_worker_baseline.rs
        └── single_instance_baseline.rs
```

ABCoder MCP reported the same module shape for `repo_name="aether-testkit"`:
one module named `aether-testkit`, package paths for each reusable module, and
one package path for each `src/bin/*_baseline.rs` binary.

## Reusable Harness Modules

Harness modules wrap production routers in a `SpawnedServer` and expose only the
base URL and port needed by tests.

```rust
// crates/aether-testkit/src/server.rs:6
pub struct SpawnedServer {
    base_url: String,
    port: u16,
    handle: tokio::task::JoinHandle<()>,
}
```

```rust
// crates/aether-testkit/src/server.rs:18
pub async fn start_on_port(port: u16, app: Router) -> Result<Self, std::io::Error> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
```

Use this pattern for new harnesses:

1. Define `SomethingHarnessConfig` for caller-selected knobs.
2. Define `SomethingHarness { server: SpawnedServer }`.
3. Provide `start(config)` and `start_on_port(config, port)`.
4. Keep the shared startup function private.
5. Forward `base_url()` and `port()` from `SpawnedServer`.

Examples already following the pattern:

- `GatewayHarness` in `crates/aether-testkit/src/gateway.rs:29`
- `ExecutionRuntimeHarness` in `crates/aether-testkit/src/execution_runtime.rs:12`
- `TunnelHarness` in `crates/aether-testkit/src/tunnel.rs:32`

## Local Dependency Modules

`postgres.rs` and `redis.rs` own temporary local process lifecycles for baseline
programs. They are not generic connection pool factories.

```rust
// crates/aether-testkit/src/postgres.rs:10
#[derive(Debug)]
pub struct ManagedPostgresServer {
    child: Option<Child>,
    postgres_bin: String,
    port: u16,
    workdir: PathBuf,
    data_dir: PathBuf,
    database_url: String,
}
```

```rust
// crates/aether-testkit/src/redis.rs:7
#[derive(Debug)]
pub struct ManagedRedisServer {
    child: Option<Child>,
    binary: String,
    port: u16,
    workdir: PathBuf,
    redis_url: String,
}
```

Guideline: local dependency helpers should expose URLs and ports, clean up in
`Drop`, and hide child-process details behind `stop()` / `restart()`.

## Probe and Assertion Modules

`load.rs` owns HTTP load probing and latency summaries. It should stay separate
from harness startup so binaries can drive any URL.

```rust
// crates/aether-testkit/src/load.rs:17
pub struct HttpLoadProbeConfig {
    pub url: String,
    pub method: Method,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub total_requests: usize,
    pub concurrency: usize,
    pub timeout: Duration,
    pub response_mode: HttpLoadProbeResponseMode,
}
```

`metrics.rs` owns small Prometheus parsing helpers and returns deterministic
`BTreeMap` labels.

```rust
// crates/aether-testkit/src/metrics.rs:3
pub struct PrometheusSample {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub value: String,
}
```

`wait.rs` owns polling primitives shared by process readiness checks.

```rust
// crates/aether-testkit/src/wait.rs:4
pub async fn wait_until<F, Fut>(
    timeout: Duration,
    poll_interval: Duration,
    mut predicate: F,
) -> bool
```

## Baseline Binaries

Longer experiment programs live under `src/bin/`. Their shape is consistent:

1. `#[tokio::main] async fn main() -> Result<(), Box<dyn std::error::Error>>`
2. `init_test_runtime_for("<suite-name>")`
3. `parse_args(...)`
4. `run_suite(&config).await?`
5. pretty JSON on stdout
6. optional JSON output file

```rust
// crates/aether-testkit/src/bin/capacity_curve_baseline.rs:111
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_test_runtime_for("capacity-curve-baseline");
    let config = parse_args(std::env::args().skip(1).collect())?;
```

Keep one-off scenario structs private inside the binary. Shared data types only
move into `src/*.rs` after two or more binaries/tests need them.

## Naming Conventions

- Harness modules use nouns matching the production surface: `gateway`,
  `execution_runtime`, `tunnel`.
- Harness config types end in `HarnessConfig`.
- Harness owners end in `Harness`.
- Managed local services end in `Managed*Server`.
- Baseline binaries end in `_baseline.rs`; the generic load CLI is
  `http_load_probe.rs`.
- Helper functions are snake_case and describe action plus target:
  `prepare_aether_postgres_schema`, `fetch_prometheus_samples`,
  `run_multi_url_http_load_probe`.

## Placement Rules

Put new code in the narrowest module:

- Router/process startup belongs in `server.rs` or a `*Harness` module.
- HTTP client defaults belong in `http.rs`.
- Reusable load logic belongs in `load.rs`, not in a baseline binary.
- Prometheus text parsing belongs in `metrics.rs`.
- Local process management belongs in `postgres.rs` or `redis.rs`.
- Binaries should orchestrate scenarios and produce reports, not define shared
  crate API.

## DON'T

Do not add broad `utils.rs` or `common.rs` modules. The existing structure uses
small domain-named files, so a generic utility module makes future placement
ambiguous.

Do not make `src/bin/*` functions public to share code. Move genuinely shared
logic into a private library module and re-export only if integration tests need
it.

Do not put production gateway behavior into this crate. It depends on
`aether-gateway` to build routers and should only wrap those routers for tests.
