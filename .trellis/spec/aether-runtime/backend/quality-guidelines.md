# Quality Guidelines

> Code quality standards for the `aether-runtime` crate.

---

## Design Posture

`aether-runtime` should stay a small infrastructure crate. Its job is to provide
safe, reusable runtime primitives for services. It must not accumulate
application policy, provider routing, database access, or Redis implementation
details.

The manifest shows the intended dependency profile:

```toml
# crates/aether-runtime/Cargo.toml:9
[dependencies]
async-stream.workspace = true
axum = { version = "0.8" }
chrono.workspace = true
futures-util.workspace = true
serde_json.workspace = true
sha2.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
uuid.workspace = true
```

New dependencies need a strong runtime-infrastructure reason. Do not add
provider, gateway, SQL, Redis, OAuth, billing, usage, or scheduler crates here.

## Visibility Rules

Keep implementation details private by default. The crate root chooses which
modules and names are public:

```rust
# crates/aether-runtime/src/lib.rs:2
mod bootstrap;
# crates/aether-runtime/src/lib.rs:4
mod config;
# crates/aether-runtime/src/lib.rs:6
mod error;
# crates/aether-runtime/src/lib.rs:8
mod observability;
# crates/aether-runtime/src/lib.rs:13
mod tracing;
```

Only re-export stable caller-facing items:

```rust
# crates/aether-runtime/src/lib.rs:35
pub use tracing::{
    init_reloadable_service_tracing, init_reloadable_tracing, LogFormat, LogReloader,
};
```

Do not make formatter structs, rolling file sinks, or internal state structs
public for test convenience. Tests already live in-module and can access private
helpers:

```rust
# crates/aether-runtime/src/tracing.rs:837
#[cfg(test)]
mod tests {
    use super::{
        bucketed_log_path, cleanup_log_files, format_target_cell, log_bucket_key,
```

## Dependency Boundary Rules

Architecture tests lock two important boundaries. First, runtime-facing crates
must not use Redis directly:

```rust
# apps/aether-gateway/src/tests/architecture/runtime_and_security.rs:105
for root in [
    "apps/aether-gateway/src",
    "crates/aether-runtime/src",
    "crates/aether-usage-runtime/src",
    "crates/aether-provider-transport/src",
] {
```

Second, `aether-runtime` must stay free of AI serving and provider policy:

```rust
# apps/aether-gateway/src/tests/architecture/ai_serving.rs:610
fn aether_runtime_stays_free_of_ai_serving_policy() {
```

DON'T add imports or strings that bind this crate to `aether-gateway`,
`aether-ai-serving`, `aether-ai-formats`, provider names, request candidates, or
finalize policy. Runtime utilities should be reusable by gateway, proxy,
testkit, and task-runtime callers.

## Concurrency Rules

Use RAII permits for in-flight accounting. `ConcurrencyPermit` increments
in-flight count on creation and decrements in `Drop`:

```rust
# crates/aether-runtime/src/concurrency.rs:137
impl ConcurrencyPermit {
    fn new(state: Arc<ConcurrencyState>, permit: OwnedSemaphorePermit) -> Self {
        let in_flight = state.in_flight.fetch_add(1, Ordering::AcqRel) + 1;
```

```rust
# crates/aether-runtime/src/concurrency.rs:159
impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        self.state.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}
```

Do not expose manual `release` methods or require callers to decrement counters.
Permit ownership is the correctness mechanism.

Use `assert!` for construction invariants that would make the primitive invalid:

```rust
# crates/aether-runtime/src/concurrency.rs:77
pub fn new(gate: &'static str, limit: usize) -> Self {
    assert!(limit > 0, "concurrency gate limit must be positive");
```

The queue follows the same positive-capacity rule:

```rust
# crates/aether-runtime/src/queue.rs:89
pub fn bounded_queue<T>(capacity: usize) -> (BoundedQueueSender<T>, BoundedQueueReceiver<T>) {
    assert!(capacity > 0, "bounded queue capacity must be positive");
```

## Admission Lifetime Rules

Request admission must hold permits until the guarded work is complete. For
futures, bind the permit before awaiting:

```rust
# crates/aether-runtime/src/admission.rs:57
pub async fn hold_admission_permit_until<T, F>(permit: Option<AdmissionPermit>, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let _permit = permit;
    future.await
}
```

For Axum responses, wrap the body stream and bind the permit inside the stream:

```rust
# crates/aether-runtime/src/admission.rs:65
fn hold_axum_response_permit(response: Response<Body>, permit: AdmissionPermit) -> Response<Body> {
    let (parts, body) = response.into_parts();
    let stream = stream! {
        let _permit = permit;
```

DON'T drop the permit immediately after building a streaming response. The
gateway finalizer relies on the helper to keep the permit alive:

```rust
# apps/aether-gateway/src/handlers/proxy/finalize.rs:168
maybe_hold_axum_response_permit(response, request_permit)
```

## Queue Accounting Rules

Use `reserve`/`try_reserve` before recording an enqueue. This avoids incrementing
depth for messages that are never accepted:

```rust
# crates/aether-runtime/src/queue.rs:110
pub async fn send(&self, value: T) -> Result<(), QueueSendError<T>> {
    let permit = match self.inner.reserve().await {
```

Only after a permit is obtained should the queue update depth and counters:

```rust
# crates/aether-runtime/src/queue.rs:120
self.record_enqueue();
permit.send(value);
```

Receiver paths must decrement depth exactly once for each delivered value:

```rust
# crates/aether-runtime/src/queue.rs:179
impl<T> BoundedQueueReceiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        let value = self.inner.recv().await?;
        self.state.depth.fetch_sub(1, Ordering::AcqRel);
```

The race-focused test is the canonical guard against depth underflow:

```rust
# crates/aether-runtime/src/queue.rs:221
#[tokio::test]
async fn send_does_not_underflow_depth_when_receiver_races() {
```

## Metrics Rules

Represent metrics as values first, then render at the edge. `MetricSample` is a
plain struct:

```rust
# crates/aether-runtime/src/metrics.rs:29
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricSample {
    pub name: &'static str,
    pub help: &'static str,
```

Snapshots own conversion to metric samples and attach stable labels:

```rust
# crates/aether-runtime/src/concurrency.rs:26
pub fn to_metric_samples(&self, gate: &'static str) -> Vec<MetricSample> {
    let labels = vec![MetricLabel::new("gate", gate)];
```

Prometheus rendering must escape label values:

```rust
# crates/aether-runtime/src/metrics.rs:126
fn escape_prometheus_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}
```

Do not emit raw payloads or high-cardinality values as labels. Gate, queue, and
service names should be stable dimensions.

## Redaction Rules

Use summaries when runtime code needs to identify a text payload without
exposing its content:

```rust
# crates/aether-runtime/src/redaction.rs:11
pub fn summarize_text_payload(text: &str) -> TextPayloadSummary {
    let digest = Sha256::digest(text.as_bytes());
```

The test shows the intended output: byte length and SHA-256 only.

```rust
# crates/aether-runtime/src/redaction.rs:27
#[test]
fn summarizes_text_payload_without_exposing_content() {
```

DON'T log request bodies, API keys, OAuth tokens, provider secrets, or raw
headers from this crate. If a caller needs diagnostics, pass summaries or stable
IDs through structured fields.

## Testing Requirements

Run crate tests for any runtime primitive change:

```bash
cargo test -p aether-runtime
```

Behavior changes should add module-local unit tests:

- `admission.rs` for future/body permit lifetime.
- `concurrency.rs` for saturation, high watermark, and release-on-drop.
- `distributed.rs` for distributed facade mapping and snapshot shape.
- `queue.rs` for depth, rejection counters, and receiver races.
- `metrics.rs` for Prometheus text and content type.
- `redaction.rs` for payload summary determinism.
- `tracing.rs` for formatter output, file rotation, and retention cleanup.

Existing examples:

```rust
# crates/aether-runtime/src/admission.rs:84
#[tokio::test]
async fn holds_permit_until_response_body_is_consumed() {
```

```rust
# crates/aether-runtime/src/concurrency.rs:185
#[test]
fn rejects_when_saturated() {
```

```rust
# crates/aether-runtime/src/metrics.rs:156
#[test]
fn escapes_prometheus_labels() {
```

For public API changes, run at least one caller check. Examples include gateway
execution runtime for admission/metrics and proxy app for tracing/shutdown.

## Code Review Checklist

Reviewers should check:

- New public names are re-exported deliberately from `lib.rs`.
- Private state stays private and is protected by ownership, atomics, or mutexes.
- Permit lifetime is tied to future completion or response body consumption.
- Rejection counters increment only on actual rejection.
- Metrics names, helps, and labels are stable and Prometheus-safe.
- Tracing setup failure is reported as `RuntimeBootstrapError`.
- Log retention cleanup remains non-fatal and emits `warn!`.
- No direct Redis, SQL, SeaORM, sqlx, gateway, provider, or AI serving dependency
  was introduced.
- Tests cover races or failure modes, not just happy paths.
