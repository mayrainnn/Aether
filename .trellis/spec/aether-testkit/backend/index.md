# aether-testkit Backend Guidelines

`aether-testkit` is the shared integration-test and baseline harness crate for
Aether's Rust services. It wraps real gateway/runtime routers, starts local
Postgres and Redis dependencies, runs HTTP pressure probes, parses Prometheus
metrics, and provides deterministic fixtures for test code.

Use these guidelines before changing `crates/aether-testkit/`, adding a new
baseline binary, or consuming its helpers from another crate.

## Pre-Development Checklist

1. Confirm the helper belongs in testkit, not in a production crate such as
   `aether-gateway`, `aether-data`, or `aether-runtime-state`.
2. Keep reusable code in private modules and expose it through `src/lib.rs`.
3. Use real production routers/builders for harnesses; do not clone production
   behavior into local fakes.
4. Bind local services only on `127.0.0.1` and prefer ephemeral ports.
5. Prove dependency readiness with a real operation and `wait_until`.
6. Keep stdout from baseline binaries as machine-readable JSON.
7. Put usage/progress text on stderr or tracing, not stdout.
8. Use `BTreeMap` for serialized report maps and parsed labels.
9. Add local unit tests for pure validation, parsing, latency math, and polling.
10. Run `cargo test -p aether-testkit` after changing source or examples that
    describe source behavior.

## Quick Source Map

```rust
// crates/aether-testkit/src/lib.rs:14
pub use execution_runtime::{ExecutionRuntimeHarness, ExecutionRuntimeHarnessConfig};
pub use fixtures::test_trace_id;
pub use gateway::{GatewayHarness, GatewayHarnessConfig};
pub use http::{json_body, test_http_client, test_http_client_config};
pub use load::{
    run_http_load_probe, run_multi_url_http_load_probe, HttpLoadProbeConfig,
    HttpLoadProbeResponseMode, HttpLoadProbeResult, MultiUrlHttpLoadProbeResult,
};
pub use metrics::{
    fetch_prometheus_samples, find_metric_value_u64, parse_prometheus_samples, PrometheusSample,
};
pub use postgres::{prepare_aether_postgres_schema, ManagedPostgresServer};
pub use redis::ManagedRedisServer;
pub use server::{reserve_local_port, SpawnedServer};
pub use tracing::{init_test_runtime, init_test_runtime_for, test_runtime_config};
pub use tunnel::{TunnelHarness, TunnelHarnessConfig};
pub use wait::wait_until;
```

The public API is a facade over small modules. Binaries under `src/bin/` are
scenario programs and should not become public API.

## Guide Index

| Guide | Use For |
| --- | --- |
| [Directory Structure](./directory-structure.md) | Module layout, facade exports, harness module shape, and baseline binary placement. |
| [Error Handling](./error-handling.md) | `String`, `std::io::Error`, and `Box<dyn Error>` boundaries; context messages; validation and worker errors. |
| [Quality Guidelines](./quality-guidelines.md) | Visibility, deterministic output, type safety, real-router harnesses, concurrency, tests, and dependency boundaries. |
| [Logging Guidelines](./logging-guidelines.md) | Runtime initialization, stdout/stderr contract, child process logs, metrics, and sensitive data rules. |
| [Database Guidelines](./database-guidelines.md) | Managed Postgres/Redis, schema preparation, fixture SQL, Redis runtime-state clients, and cleanup. |

## Applicable Database Scope

`database-guidelines.md` is intentionally retained. This crate owns
`ManagedPostgresServer`, `ManagedRedisServer`, `prepare_aether_postgres_schema`,
and baseline-local SQL fixture setup. It must document database rules because
future agents will otherwise miss the distinction between disposable test data
and production schema ownership.

## Architecture Evidence

GitNexus MCP resources for `repo="Aether"` reported the indexed Aether codebase
has 3,140 files, 83,229 symbols, and 300 execution flows. The relevant clusters
for this crate are `Tests` and `Execution_runtime`; those clusters connect the
testkit's harnesses and baselines to gateway/runtime/tunnel execution behavior.

ABCoder MCP was run against an isolated `aether-testkit` AST with
`repo_name="aether-testkit"`. It reported one module named `aether-testkit`,
package paths for every reusable module, and package paths for each baseline
binary. Its `get_file_structure` output confirmed the key public nodes:

- `SpawnedServer`, `SpawnedServer::start`, and `SpawnedServer::start_on_port`
- `GatewayHarness`, `GatewayHarnessConfig`, and `GatewayHarness::start_with_server`
- `ExecutionRuntimeHarness` and `ExecutionRuntimeHarnessConfig`
- `TunnelHarness` and `TunnelHarnessConfig`
- `ManagedPostgresServer` and `prepare_aether_postgres_schema`
- `ManagedRedisServer`
- `HttpLoadProbeConfig`, `HttpLoadProbeResult`, and `run_http_load_probe`
- `PrometheusSample`, `fetch_prometheus_samples`, and `find_metric_value_u64`
- `wait_until`
- `init_test_runtime_for`

ABCoder `get_ast_node` also showed important references: `SpawnedServer` is used
by gateway, execution-runtime, and tunnel harnesses; `ManagedRedisServer` is
referenced by multiple failure/recovery and admission baselines; and
`run_http_load_probe` is consumed by capacity, gateway/tunnel stream, single
instance, owner-relay, and CLI probe binaries.

## Non-Applicable Guides

No template guide was deleted. The original five backend guides all apply to
this crate after adapting them to reality.

## Quality Check

Before finishing documentation or source work here:

1. Search this directory for legacy template markers and unfinished filler text;
   the scan should return nothing.
2. `find .trellis/spec/aether-testkit/backend -maxdepth 1 -type f -name "*.md"`
   should list only the files in the Guide Index above.
3. Every guide should cite real `crates/aether-testkit/...` source paths and
   line numbers.
4. Each guide should include at least one DON'T or explicit forbidden pattern.
5. `cargo test -p aether-testkit` should pass when source behavior has changed.
6. For spec-only edits, at minimum run line-count and template-residue checks.

## Current Dependency Shape

`aether-testkit` intentionally depends on application/runtime/data crates because
it is an application-layer test utility crate.

```toml
# crates/aether-testkit/Cargo.toml:11
aether-data.workspace = true
aether-contracts.workspace = true
aether-gateway.workspace = true
aether-http.workspace = true
aether-runtime.workspace = true
aether-runtime-state.workspace = true
```

Do not add new dependencies unless the requested test surface cannot be built
from existing workspace crates or the standard library.
