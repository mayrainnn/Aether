# Directory Structure

> How backend code is organized in the `aether-runtime` crate.

---

## Overview

`aether-runtime` is a small Rust crate under `crates/aether-runtime/`. It uses a
facade root plus focused single-file modules. Public modules are exposed only
when callers need module-qualified access, while config/bootstrap/error/tracing
internals stay private and are re-exported through stable names.

The module map is explicit in `src/lib.rs`:

```rust
# crates/aether-runtime/src/lib.rs:1
pub mod admission;
mod bootstrap;
pub mod concurrency;
mod config;
pub mod distributed;
mod error;
pub mod metrics;
mod observability;
pub mod queue;
pub mod redaction;
pub mod shutdown;
pub mod task;
mod tracing;
```

Use this shape for new runtime utilities: one cohesive file per utility family,
with only stable caller-facing types re-exported from `lib.rs`.

## Actual Layout

```text
crates/aether-runtime/
|-- Cargo.toml
`-- src/
    |-- admission.rs      # holds local/distributed permits through futures and axum bodies
    |-- bootstrap.rs      # init_service_runtime orchestration
    |-- concurrency.rs    # in-process semaphore gate plus metrics snapshots
    |-- config.rs         # ServiceRuntimeConfig builder
    |-- distributed.rs    # in-memory distributed gate compatibility facade
    |-- error.rs          # RuntimeBootstrapError
    |-- lib.rs            # public facade and module visibility
    |-- metrics.rs        # MetricSample model and Prometheus response rendering
    |-- observability.rs  # log destination, rotation, and file logging config
    |-- queue.rs          # bounded Tokio mpsc wrapper with queue metrics
    |-- redaction.rs      # text payload byte/hash summaries
    |-- shutdown.rs       # cross-platform shutdown signal wait
    |-- task.rs           # named tokio task spawn helper
    `-- tracing.rs        # tracing subscriber, formatters, rolling file sink
```

There are no nested module directories today. Do not create a tree of folders
unless a utility family outgrows one file and has multiple private subparts.

## Public Facade Pattern

Callers should normally import from `aether_runtime::{...}` rather than from
private implementation modules. The facade re-exports the stable contract:

```rust
# crates/aether-runtime/src/lib.rs:18
pub use bootstrap::init_service_runtime;
# crates/aether-runtime/src/lib.rs:20
pub use config::ServiceRuntimeConfig;
# crates/aether-runtime/src/lib.rs:27
pub use observability::{
    FileLoggingConfig, LogDestination, LogRotation, ServiceObservabilityConfig,
};
```

Some modules remain `pub mod` because callers need namespaced access or the
submodule is part of the conceptual API. `task::spawn_named` is imported this
way by `aether-task-runtime`:

```rust
# crates/aether-task-runtime/src/lib.rs:3
use aether_runtime::task::spawn_named;
```

Keep new public APIs on the facade when they are general runtime helpers.
Expose a full `pub mod` only when the namespace itself is useful and stable.

## Module Responsibilities

`admission.rs` owns permit lifetime composition. It wraps local
`ConcurrencyPermit` and optional distributed permits in one `AdmissionPermit`:

```rust
# crates/aether-runtime/src/admission.rs:8
pub struct AdmissionPermit {
    _local: Option<ConcurrencyPermit>,
    _distributed: Option<Box<dyn Send + Sync>>,
}
```

It is also the only module that knows how to hold a permit until an Axum response
body is drained:

```rust
# crates/aether-runtime/src/admission.rs:65
fn hold_axum_response_permit(response: Response<Body>, permit: AdmissionPermit) -> Response<Body> {
```

`concurrency.rs` owns in-process semaphore gates and snapshots. The private
state is reference-counted and shared by permits:

```rust
# crates/aether-runtime/src/concurrency.rs:61
#[derive(Debug)]
struct ConcurrencyState {
    gate: &'static str,
    limit: usize,
```

`distributed.rs` currently adapts that local gate behind a distributed-looking
contract. It is an in-memory implementation, not a Redis or database layer:

```rust
# crates/aether-runtime/src/distributed.rs:77
impl DistributedConcurrencyGate {
    pub fn new_in_memory(gate: &'static str, limit: usize) -> Self {
```

`queue.rs` wraps Tokio `mpsc` channels to add depth, rejection, and high
watermark metrics. It owns both sender and receiver wrappers:

```rust
# crates/aether-runtime/src/queue.rs:77
#[derive(Debug, Clone)]
pub struct BoundedQueueSender<T> {
```

`metrics.rs` defines the local metric model and renders Prometheus text. It does
not register collectors or own a metrics server:

```rust
# crates/aether-runtime/src/metrics.rs:29
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricSample {
```

`tracing.rs` is the only large module. It contains subscriber initialization,
pretty/json formatters, reload support, rolling file writing, and retention
cleanup. Keep this complexity centralized so services only build config objects.

## Caller Organization

Higher layers are consumers. The gateway execution runtime composes request
gates, distributed runtime-state gates, admission permits, and metrics:

```rust
# apps/aether-gateway/src/execution_runtime/server.rs:91
async fn try_acquire_request_permit(
    &self,
) -> Result<Option<AdmissionPermit>, RequestAdmissionError> {
```

The gateway finalizer holds admission permits through streaming responses:

```rust
# apps/aether-gateway/src/handlers/proxy/finalize.rs:168
maybe_hold_axum_response_permit(response, request_permit)
```

The proxy app builds service runtime config and initializes reloadable tracing:

```rust
# apps/aether-proxy/src/config.rs:788
pub fn service_runtime_config(&self) -> anyhow::Result<ServiceRuntimeConfig> {
```

Do not move caller-specific decisions back into `aether-runtime`. The crate
should not know about gateway routes, provider formats, request candidates, or
proxy tunnel policy.

## Naming Conventions

Use precise runtime nouns:

- `ConcurrencyGate`, `ConcurrencyPermit`, and `ConcurrencySnapshot` for local
  semaphore-backed admission.
- `DistributedConcurrencyGate`, `DistributedConcurrencyPermit`, and
  `DistributedConcurrencySnapshot` for the compatibility abstraction.
- `BoundedQueueSender`, `BoundedQueueReceiver`, `QueueSnapshot`, and
  `QueueSendError` for channel wrappers.
- `ServiceRuntimeConfig` and `ServiceObservabilityConfig` for service-level
  bootstrap settings.
- `LogDestination`, `LogRotation`, `LogFormat`, and `LogReloader` for tracing.

The code uses static names for metric labels and gate/queue names because those
labels are long-lived telemetry dimensions:

```rust
# crates/aether-runtime/src/concurrency.rs:26
pub fn to_metric_samples(&self, gate: &'static str) -> Vec<MetricSample> {
# crates/aether-runtime/src/queue.rs:19
pub fn to_metric_samples(&self, queue: &'static str) -> Vec<MetricSample> {
```

## Placement Rules

Add new files only when the behavior forms a new runtime utility family. Good
fits for this crate include:

- Process lifecycle helpers.
- Tokio task, queue, or semaphore helpers.
- Observability helpers that are independent of one application.
- Metrics data models and response rendering.
- Redaction helpers that are safe for logs or metrics.

Do not add files for:

- SQL/Redis state backends. Put those in `aether-runtime-state` or `aether-data`.
- Provider routing or model selection. Put those in AI serving or gateway code.
- Admin handlers or HTTP route trees. Put those in application crates.
- Business workflow workers. Put those in the owning service crate.

## Testing Layout

Unit tests live inside the module they protect. This allows tests to exercise
private helpers without widening visibility:

```rust
# crates/aether-runtime/src/queue.rs:193
#[cfg(test)]
mod tests {
    use super::{bounded_queue, QueueSendError};
```

The tracing module follows the same pattern for private formatter and rolling
file helpers:

```rust
# crates/aether-runtime/src/tracing.rs:837
#[cfg(test)]
mod tests {
    use super::{
        bucketed_log_path, cleanup_log_files, format_target_cell, log_bucket_key,
```

Prefer module-local unit tests for new internals, and add caller tests only when
the public facade behavior changes for gateway, proxy, or task-runtime callers.
