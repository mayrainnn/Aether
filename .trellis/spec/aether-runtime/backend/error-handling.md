# Error Handling

> How errors are handled in the `aether-runtime` crate.

---

## Overview

`aether-runtime` uses small typed errors at public boundaries and standard
library errors for filesystem or signal operations. It does not use `anyhow` in
the crate itself. Callers may convert runtime errors into their own API or
application errors.

The crate-level bootstrap error is deliberately narrow:

```rust
# crates/aether-runtime/src/error.rs:1
#[derive(Debug, thiserror::Error)]
pub enum RuntimeBootstrapError {
    #[error("failed to initialize tracing: {0}")]
    Tracing(String),
}
```

Concurrency and distributed admission use domain-specific enums so callers can
differentiate saturation from infrastructure failure:

```rust
# crates/aether-runtime/src/concurrency.rs:8
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConcurrencyError {
    #[error("concurrency gate {gate} is saturated at {limit}")]
    Saturated { gate: &'static str, limit: usize },
```

## Error Types

### RuntimeBootstrapError

`RuntimeBootstrapError` wraps tracing initialization failures. Bootstrap calls
initialize tracing first and metrics second:

```rust
# crates/aether-runtime/src/bootstrap.rs:4
pub fn init_service_runtime(config: ServiceRuntimeConfig) -> Result<(), RuntimeBootstrapError> {
    crate::tracing::init_tracing(config.clone())?;
    crate::metrics::init_metrics(config);
    Ok(())
}
```

Because metrics initialization is an idempotent `OnceLock` set, the only current
bootstrap failure class is tracing.

### ConcurrencyError

`ConcurrencyError` has two variants:

```rust
# crates/aether-runtime/src/concurrency.rs:10
Saturated { gate: &'static str, limit: usize },
# crates/aether-runtime/src/concurrency.rs:12
Closed { gate: &'static str },
```

`try_acquire` increments rejection metrics only for `NoPermits`, then returns
`Saturated`:

```rust
# crates/aether-runtime/src/concurrency.rs:104
pub fn try_acquire(&self) -> Result<ConcurrencyPermit, ConcurrencyError> {
    match self.state.semaphore.clone().try_acquire_owned() {
# crates/aether-runtime/src/concurrency.rs:107
        Err(tokio::sync::TryAcquireError::NoPermits) => {
            self.state.rejected.fetch_add(1, Ordering::Relaxed);
            Err(ConcurrencyError::Saturated {
```

`acquire` waits for capacity and maps semaphore closure to `Closed`:

```rust
# crates/aether-runtime/src/concurrency.rs:91
pub async fn acquire(&self) -> Result<ConcurrencyPermit, ConcurrencyError> {
    let permit = self
        .state
        .semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ConcurrencyError::Closed {
```

### DistributedConcurrencyError

The distributed facade mirrors saturation and adds unavailable/configuration
variants:

```rust
# crates/aether-runtime/src/distributed.rs:6
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DistributedConcurrencyError {
    #[error("distributed concurrency gate {gate} is saturated at {limit}")]
    Saturated { gate: &'static str, limit: usize },
    #[error("distributed concurrency gate {gate} is unavailable: {message}")]
```

The in-memory implementation converts local `ConcurrencyError` variants without
losing the gate name:

```rust
# crates/aether-runtime/src/distributed.rs:103
self.state
    .gate_impl
    .try_acquire()
    .map(|permit| DistributedConcurrencyPermit { _permit: permit })
    .map_err(|err| match err {
```

### QueueSendError

Queue send failures retain ownership of the unsent value:

```rust
# crates/aether-runtime/src/queue.rs:71
#[derive(Debug)]
pub enum QueueSendError<T> {
    Full(T),
    Closed(T),
}
```

This is intentional. Callers can retry, downgrade priority, or record the
payload elsewhere. Do not replace this with a string error that drops the value.

## Propagation Patterns

Use `?` for setup paths where the underlying error should abort the operation.
Examples:

```rust
# crates/aether-runtime/src/tracing.rs:669
fs::create_dir_all(&config.dir)?;
# crates/aether-runtime/src/tracing.rs:679
let file = open_bucketed_log_file(&config.dir, service_name, &current_bucket)?;
```

Use `map_err` when converting lower-level errors into runtime public errors:

```rust
# crates/aether-runtime/src/tracing.rs:582
.map_err(|err| RuntimeBootstrapError::Tracing(err.to_string()))?;
```

Use explicit `match` when the error variant affects metrics:

```rust
# crates/aether-runtime/src/queue.rs:125
pub fn try_send(&self, value: T) -> Result<(), QueueSendError<T>> {
    let permit = match self.inner.try_reserve() {
# crates/aether-runtime/src/queue.rs:128
        Err(mpsc::error::TrySendError::Full(_)) => {
            self.state
                .rejected_full_total
```

## Caller Error Mapping

Application crates map runtime errors into HTTP or service-specific errors.
The gateway execution runtime maps local and distributed saturation to an
overloaded server error:

```rust
# apps/aether-gateway/src/execution_runtime/server.rs:258
async fn acquire_request_permit(
    state: &ExecutionRuntimeAppState,
) -> Result<Option<AdmissionPermit>, ExecutionRuntimeAppError> {
# apps/aether-gateway/src/execution_runtime/server.rs:263
    Err(RequestAdmissionError::Local(ConcurrencyError::Saturated { gate, limit }))
```

This boundary is correct. `aether-runtime` should not know which HTTP status code
or JSON error shape the gateway wants.

The proxy app treats tracing and signal setup as fatal at startup:

```rust
# apps/aether-proxy/src/app.rs:715
let reloader = init_reloadable_service_tracing(
    &config.log_level,
    config
        .service_runtime_config()
        .expect("proxy service runtime config should be valid"),
)
.expect("proxy tracing should initialize");
```

## File Logging Errors

File sink setup is fail-loud. If `LogDestination::File` or `Both` is configured
without file logging config, initialization returns `RuntimeBootstrapError`:

```rust
# crates/aether-runtime/src/tracing.rs:500
if config.observability.log_destination.needs_file_sink() {
    let Some(file_logging) = config.observability.file_logging.clone() else {
        return Err(RuntimeBootstrapError::Tracing(
            "file logging requires a configured log directory".to_string(),
        ));
```

Retention cleanup is different. Startup cleanup failure is surfaced as a warning
but does not block log sink creation:

```rust
# crates/aether-runtime/src/tracing.rs:670
let startup_cleanup_warning =
    cleanup(service_name, &config)
        .err()
        .map(|err| StartupCleanupWarning {
```

The behavior is covered by a unit test:

```rust
# crates/aether-runtime/src/tracing.rs:965
fn rolling_file_sink_treats_startup_cleanup_failure_as_non_fatal() {
```

## Shutdown Errors

`wait_for_shutdown_signal` returns `std::io::Error` because signal handler
registration can fail:

```rust
# crates/aether-runtime/src/shutdown.rs:1
#[cfg(unix)]
pub async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut terminate = signal(SignalKind::terminate())?;
```

Callers decide whether this is fatal. The proxy treats it as a startup/runtime
failure:

```rust
# apps/aether-proxy/src/app.rs:725
async fn wait_for_shutdown() {
    wait_for_shutdown_signal()
        .await
        .expect("failed to install shutdown signal handler");
```

## Common Mistakes

DON'T convert `QueueSendError<T>` into a bare string before the caller has a
chance to recover the value.

```rust
# crates/aether-runtime/src/queue.rs:132
return Err(QueueSendError::Full(value));
```

DON'T silently ignore gate saturation. Saturation is load-shedding and must
surface to the caller with the gate name and limit.

```rust
# crates/aether-runtime/src/concurrency.rs:109
Err(ConcurrencyError::Saturated {
    gate: self.state.gate,
    limit: self.state.limit,
})
```

DON'T make retention cleanup failure fatal. The code intentionally logs a
warning after startup succeeds:

```rust
# crates/aether-runtime/src/tracing.rs:584
if let Some(warning) = startup_cleanup_warning.as_ref() {
    emit_log_cleanup_warning("startup", warning.log_dir.as_path(), &warning.error);
}
```

DON'T add `anyhow::Error` to public runtime APIs. Public errors in this crate are
small typed enums or standard IO errors so higher layers can map them precisely.
