# Logging Guidelines

> How logging is done in the `aether-runtime` crate.

---

## Overview

`aether-runtime` owns service tracing initialization and custom log formatting
for Rust services. It uses `tracing` and `tracing-subscriber`, with two output
formats and three destinations:

```rust
# crates/aether-runtime/src/tracing.rs:28
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}
```

```rust
# crates/aether-runtime/src/observability.rs:4
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogDestination {
    Stdout,
    File,
    Both,
}
```

Services should build `ServiceRuntimeConfig` and call `init_service_runtime` or
`init_reloadable_service_tracing`; they should not duplicate subscriber setup.

## Service Configuration

`ServiceRuntimeConfig::new` sets the service name, default filter, and default
pretty stdout observability:

```rust
# crates/aether-runtime/src/config.rs:10
impl ServiceRuntimeConfig {
    pub const fn new(service_name: &'static str, default_log_filter: &'static str) -> Self {
        Self {
            service_name,
            default_log_filter,
            observability: ServiceObservabilityConfig::new(crate::LogFormat::Pretty, service_name),
```

Gateway and proxy callers attach service identity fields:

```rust
# apps/aether-gateway/src/main.rs:857
let config = self
    .logging
    .apply_to_runtime_config(ServiceRuntimeConfig::new(
        "aether-gateway",
        default_log_filter,
    ))?;
Ok(config
    .with_node_role(self.node_role.as_str())
    .with_instance_id(resolve_gateway_log_instance_id()))
```

```rust
# apps/aether-proxy/src/config.rs:789
let mut config = ServiceRuntimeConfig::new("aether-proxy", "aether_proxy=info")
    .with_log_format(aether_runtime::LogFormat::Pretty)
    .with_log_destination(self.log_destination.into())
    .with_node_role("proxy")
    .with_instance_id(self.node_name.trim().to_string());
```

## Subscriber Initialization

`init_tracing` is one-time global initialization guarded by `OnceLock`:

```rust
# crates/aether-runtime/src/tracing.rs:24
static TRACING_INIT: OnceLock<Result<(), String>> = OnceLock::new();
```

It reads `EnvFilter` from the environment and falls back to the service default:

```rust
# crates/aether-runtime/src/tracing.rs:381
pub(crate) fn init_tracing(config: ServiceRuntimeConfig) -> Result<(), RuntimeBootstrapError> {
    TRACING_INIT
        .get_or_init(|| {
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.default_log_filter.into());
```

Reloadable tracing is separate and returns a boxed reloader closure:

```rust
# crates/aether-runtime/src/tracing.rs:490
pub fn init_reloadable_service_tracing(
    initial_filter: &str,
    config: ServiceRuntimeConfig,
) -> Result<LogReloader, RuntimeBootstrapError> {
```

The proxy stores this reloader for runtime log-level updates:

```rust
# apps/aether-proxy/src/app.rs:714
fn init_tracing(config: &Config) {
    let reloader = init_reloadable_service_tracing(
        &config.log_level,
```

## Pretty Format

Pretty logs are custom-formatted. They include timestamp, level, target, message,
and remaining structured fields. The formatter intentionally omits compact
service identity fields from pretty output:

```rust
# crates/aether-runtime/src/tracing.rs:146
#[derive(Debug, Clone)]
struct PrettyRuntimeEventFormatter {
    _identity: RuntimeLogIdentity,
    ansi: bool,
}
```

The pretty formatter writes a fixed-width target cell and span tree prefix:

```rust
# crates/aether-runtime/src/tracing.rs:192
let target_cell = format_target_cell(meta.target(), TARGET_COLUMN_WIDTH);
write_colored(&mut writer, &target_cell, self.ansi.then_some(ANSI_CYAN))?;
# crates/aether-runtime/src/tracing.rs:198
let prefix = span_tree_prefix(depth);
```

Tests lock the expected shape:

```rust
# crates/aether-runtime/src/tracing.rs:990
fn pretty_formatter_omits_service_identity_fields() {
```

```rust
# crates/aether-runtime/src/tracing.rs:1068
fn pretty_formatter_adds_tree_prefix_inside_span() {
```

Use pretty format for local operator readability. Do not add service identity
text to pretty output unless all caller tests and log snapshots are updated.

## JSON Format

JSON logs include service identity and nest event fields under `fields`:

```rust
# crates/aether-runtime/src/tracing.rs:240
let mut payload = Map::new();
payload.insert(
    "timestamp".to_string(),
    Value::String(formatted_timestamp()),
);
payload.insert("level".to_string(), Value::String(meta.level().to_string()));
payload.insert(
    "service".to_string(),
    Value::String(self.identity.service.to_string()),
);
```

The formatter also records span depth:

```rust
# crates/aether-runtime/src/tracing.rs:270
payload.insert("span_depth".to_string(), Value::from(depth as u64));
payload.insert(
    "fields".to_string(),
    Value::Object(fields.into_json_object()),
);
```

Tests assert identity fields and nested event fields:

```rust
# crates/aether-runtime/src/tracing.rs:1151
fn json_formatter_includes_service_identity_fields() {
```

Use JSON format for machine ingestion. Keep field names stable:
`timestamp`, `level`, `service`, `node_role`, `instance_id`, `target`,
`span_depth`, and `fields`.

## Log Levels

The crate itself uses a small set of log levels:

- `debug!` for low-volume runtime task lifecycle events.
- `warn!` for non-fatal operational problems.
- Formatter tests use `info!`, `warn!`, `debug!`, and `info_span!` to verify
  output; those test events are examples of formatting, not application policy.

The task spawn helper uses `debug!`:

```rust
# crates/aether-runtime/src/task.rs:8
tokio::spawn(async move {
    tracing::debug!(task = task_name, "spawned runtime task");
    future.await
})
```

Log retention cleanup failures use `warn!` with structured fields:

```rust
# crates/aether-runtime/src/tracing.rs:756
fn emit_log_cleanup_warning(phase: &'static str, log_dir: &Path, error: &impl std::fmt::Display) {
    tracing::warn!(
        event_name = "log_retention_cleanup_failed",
        log_type = "ops",
        phase,
```

Do not use `error!` for cleanup failures that the code intentionally treats as
non-fatal. Reserve caller-side `error!` for failed operations that require
operator action or abort work.

## Structured Fields

Prefer stable key-value fields over interpolated prose. The crate already uses:

```rust
# crates/aether-runtime/src/tracing.rs:757
tracing::warn!(
    event_name = "log_retention_cleanup_failed",
    log_type = "ops",
    phase,
    log_dir = %log_dir.display(),
    error = %error,
    "log retention cleanup failed"
);
```

Task-level logs use a stable `task` field:

```rust
# crates/aether-runtime/src/task.rs:9
tracing::debug!(task = task_name, "spawned runtime task");
```

For new runtime logs, use fields such as `event_name`, `log_type`, `task`,
`phase`, `gate`, `queue`, `service`, and `error`. Avoid free-form strings that
operators cannot query.

## File Logging

File logging requires a directory, rotation, retention days, and max files:

```rust
# crates/aether-runtime/src/observability.rs:23
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLoggingConfig {
    pub dir: PathBuf,
    pub rotation: LogRotation,
    pub retention_days: u64,
    pub max_files: usize,
}
```

The gateway validates that a log directory exists when file output is selected:

```rust
# apps/aether-gateway/src/main.rs:613
if matches!(
    self.log_destination,
    GatewayLogDestinationArg::File | GatewayLogDestinationArg::Both
) {
```

The rolling sink creates directories and opens bucketed append files:

```rust
# crates/aether-runtime/src/tracing.rs:664
fn new_with_cleanup(
    service_name: &'static str,
    config: FileLoggingConfig,
    cleanup: fn(&str, &FileLoggingConfig) -> io::Result<usize>,
) -> io::Result<(Self, Option<StartupCleanupWarning>)> {
    fs::create_dir_all(&config.dir)?;
```

Bucket names are daily or hourly:

```rust
# crates/aether-runtime/src/tracing.rs:729
fn log_bucket_key<Tz>(rotation: LogRotation, now: DateTime<Tz>) -> String
where
    Tz: TimeZone,
```

Background cleanup runs only if a Tokio runtime is present:

```rust
# crates/aether-runtime/src/tracing.rs:740
fn spawn_log_cleanup_task(service_name: &'static str, config: FileLoggingConfig) {
    if tokio::runtime::Handle::try_current().is_err() {
        return;
    }
```

## What Not To Log

Do not log secrets or raw request payloads in this crate. This includes API keys,
OAuth tokens, provider credentials, raw HTTP bodies, authorization headers, and
tenant-specific secret values.

Use the redaction helper when a caller needs a stable payload fingerprint:

```rust
# crates/aether-runtime/src/redaction.rs:5
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPayloadSummary {
    pub bytes: usize,
    pub sha256: String,
}
```

DON'T add payload text to metrics labels or structured log fields. Metrics and
logs emitted from this crate should use stable operational dimensions such as
task names, gate names, queue names, phases, paths to log directories, and error
messages.
