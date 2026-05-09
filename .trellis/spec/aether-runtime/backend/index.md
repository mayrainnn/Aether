# Backend Development Guidelines

> Entry point for backend work in the `aether-runtime` crate.

---

## Package Summary

`aether-runtime` is the shared runtime infrastructure crate for Aether Rust
services. It provides Tokio-facing building blocks for service bootstrap,
tracing, shutdown signals, in-process concurrency gates, admission permit
lifetimes, bounded queues, Prometheus text rendering, rolling log files, and
safe payload summaries.

Evidence:

```toml
# crates/aether-runtime/Cargo.toml:2
name = "aether-runtime"
# crates/aether-runtime/Cargo.toml:7
description = "Shared runtime/bootstrap helpers for Aether Rust services"
```

The crate is intentionally low in the dependency graph. Its manifest depends on
workspace Tokio/tracing utilities plus `axum` for response bodies and metrics,
but it must not depend on provider, gateway, data, or AI serving crates.

```rust
# crates/aether-runtime/src/lib.rs:15
pub use admission::{
    hold_admission_permit_until, maybe_hold_axum_response_permit, AdmissionPermit,
};
# crates/aether-runtime/src/lib.rs:18
pub use bootstrap::init_service_runtime;
# crates/aether-runtime/src/lib.rs:19
pub use concurrency::{ConcurrencyError, ConcurrencyGate, ConcurrencyPermit, ConcurrencySnapshot};
```

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Crate layout, module ownership, public facade exports, and caller boundaries | Filled |
| [Error Handling](./error-handling.md) | Runtime bootstrap errors, gate saturation, queue send errors, IO propagation, and caller mapping | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Visibility, dependency boundaries, concurrency accounting, metrics, redaction, and required tests | Filled |
| [Logging Guidelines](./logging-guidelines.md) | `tracing` formatter behavior, structured fields, log levels, file logging, retention warnings, and redaction boundaries | Filled |

`database-guidelines.md` was removed because this crate has no database, ORM,
Redis, migration, transaction, connection-pool, or query code. Runtime Redis
semantics belong to `aether-runtime-state`, and SQL/storage semantics belong to
`aether-data`.

## Pre-Development Checklist

Before editing `crates/aether-runtime/`, verify:

- The change is runtime infrastructure, not gateway policy or provider routing.
- The public API can remain small and explicit through `src/lib.rs`.
- New state uses owned Tokio primitives, `Arc`, atomics, or private structs, not
  process-wide mutable globals except for documented one-time initialization.
- Backpressure or admission changes preserve permit lifetime until the guarded
  future or response body is finished.
- Metrics remain pure `MetricSample` values and Prometheus text rendering stays
  framework-neutral except for the final `axum::Response<Body>` helper.
- Logs do not include request payloads, API keys, tokens, or model/provider
  secrets. Use `summarize_text_payload` for byte count plus hash when needed.
- File logging continues to fail loudly during sink setup but treats retention
  cleanup failure as a warning.
- There is no direct Redis, SQL, SeaORM, sqlx, provider transport, or gateway
  business-policy dependency.

## Public Contract

The crate facade exposes stable utility types and functions from private or
focused modules:

```rust
# crates/aether-runtime/src/lib.rs:25
pub use error::RuntimeBootstrapError;
# crates/aether-runtime/src/lib.rs:26
pub use metrics::{prometheus_response, service_up_sample, MetricKind, MetricLabel, MetricSample};
# crates/aether-runtime/src/lib.rs:30
pub use queue::{
    bounded_queue, BoundedQueueReceiver, BoundedQueueSender, QueueSendError, QueueSnapshot,
};
```

Higher layers consume these helpers directly. For example, the gateway execution
runtime builds request gates and Prometheus samples:

```rust
# apps/aether-gateway/src/execution_runtime/server.rs:64
async fn metric_samples(&self) -> Vec<MetricSample> {
    let mut samples = vec![service_up_sample(EXECUTION_RUNTIME_COMPONENT)];
```

The proxy app uses reloadable service tracing and the shared shutdown signal:

```rust
# apps/aether-proxy/src/app.rs:714
fn init_tracing(config: &Config) {
    let reloader = init_reloadable_service_tracing(
# apps/aether-proxy/src/app.rs:725
async fn wait_for_shutdown() {
    wait_for_shutdown_signal()
```

## Boundary Rules

The crate is runtime infrastructure only. The architecture test suite locks this
boundary:

```rust
# apps/aether-gateway/src/tests/architecture/ai_serving.rs:610
fn aether_runtime_stays_free_of_ai_serving_policy() {
    let runtime_manifest = read_workspace_file("crates/aether-runtime/Cargo.toml");
```

Do not add AI routing or provider concepts such as `ExecutionPlan`, `OpenAI`,
`Claude`, `Gemini`, `request_candidate`, or finalize policy to this crate. Keep
those in application and domain crates.

Runtime Redis is also outside this crate:

```rust
# apps/aether-gateway/src/tests/architecture/runtime_and_security.rs:105
for root in [
    "apps/aether-gateway/src",
    "crates/aether-runtime/src",
```

That test rejects direct `redis::cmd`, `redis::Script`, and data-layer Redis
drivers in runtime-facing crates. If a feature needs distributed locks or
semaphores, add it to `aether-runtime-state` and keep this crate on the local,
generic API side.

## Quality Gate

Minimum verification for spec or code changes in this crate:

```bash
cargo test -p aether-runtime
```

Also scan the spec directory for template residue before reporting completion.

For public API changes, also compile or test at least one caller path that uses
the touched API:

- `apps/aether-gateway/src/execution_runtime/server.rs` for admission,
  concurrency, and metrics behavior.
- `apps/aether-gateway/src/handlers/proxy/finalize.rs` for response permit
  lifetime behavior.
- `apps/aether-proxy/src/app.rs` and `apps/aether-proxy/src/config.rs` for
  service tracing and shutdown wiring.
- `crates/aether-task-runtime/src/lib.rs` for `task::spawn_named`.

## Review Focus

Reviewers should spend most time on:

- Public facade stability in `src/lib.rs`.
- Concurrency permit ownership and `Drop` accounting.
- Queue depth and high-watermark accounting under receiver races.
- File logging setup errors versus retention cleanup warnings.
- Absence of secrets or raw payloads in logs and metrics.
- One-time tracing initialization behavior.
- Boundary drift into database, Redis, provider, or gateway policy code.

## Non-Goals

This spec intentionally does not cover:

- SQL repositories, migrations, or SeaORM entities.
- Redis-backed distributed runtime state.
- Provider routing, AI format compatibility, or request candidate selection.
- Admin API handler structure.
- Frontend behavior.

Load the package-specific spec for those areas instead of adding their rules to
`aether-runtime`.
