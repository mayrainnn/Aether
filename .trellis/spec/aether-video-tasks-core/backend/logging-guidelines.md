# Logging Guidelines

`aether-video-tasks-core` currently has no `tracing` dependency and no logging
macros. That is intentional for this crate. It is a deterministic domain
library that returns structured decisions to callers; the gateway and runtime
layers decide what to log.

## Current Logging Surface

The source contains no `trace!`, `debug!`, `info!`, `warn!`, `error!`, or
`#[tracing::instrument]` usage under `crates/aether-video-tasks-core/src/`.
Keep it that way unless the crate starts owning an external side effect whose
failure cannot be communicated through an existing return type.

The service returns data, not log events:

```rust
// crates/aether-video-tasks-core/src/service.rs:162
pub fn project_openai_task_response(
    &self,
    task_id: &str,
    provider_body: &Map<String, Value>,
) -> bool {
    if self.truth_source_mode != VideoTaskTruthSourceMode::RustAuthoritative {
        return false;
    }
    self.store.project_openai(task_id, provider_body)
}
```

The read side propagates `DataLayerError` rather than logging it:

```rust
// crates/aether-video-tasks-core/src/read_side.rs:38
let Some(task) = state.find_stored_video_task(lookup).await? else {
    return Ok(None);
};
```

## Trace Context Is Carried In Plans

When the crate needs a trace identifier, it stores it in the generated
`ExecutionPlan.request_id`. That lets outer runtime layers correlate execution
without instrumenting this crate.

```rust
// crates/aether-video-tasks-core/src/openai.rs:379
Some(ExecutionPlan {
    request_id: trace_id.to_string(),
    candidate_id: None,
    provider_name: self.transport.provider_name.clone(),
    provider_id: self.transport.provider_id.clone(),
```

```rust
// crates/aether-video-tasks-core/src/service.rs:227
self.store
    .list_active_snapshots(limit)
    .into_iter()
    .enumerate()
    .filter_map(|(index, snapshot)| {
        let trace_id = format!("{trace_prefix}-{index}");
```

If you need more correlation, extend the structured data passed to the caller
instead of adding ad hoc logging inside mapping functions.

## Caller-Level Log Shape

At application boundaries, the wider project uses structured `tracing` fields.
Follow that style in callers if a video-task integration needs observability.

```rust
// apps/aether-gateway/src/state/integrations.rs:167
if let Err(err) = self
    .runtime_state
    .kv_set(
        &cache_key,
        serialized,
        Some(std::time::Duration::from_secs(
            model_fetch_interval_minutes().saturating_mul(60),
        )),
    )
    .await
{
    debug!(
        provider_id = %provider_id,
        key_id = %key_id,
        error = %err,
        "gateway model fetch cache write failed"
    );
}
```

For video task callers, good fields are `request_id`, `provider_id`,
`endpoint_id`, `key_id`, `route_family`, `report_kind`, `truth_source_mode`, and
whether a plan was built. Avoid logging full request or provider bodies.

## Level Guidance For Callers

Use `debug` for recoverable cache, registry, or projection misses where the
caller can continue with a fallback.

Use `info` for lifecycle milestones that operators intentionally track, such as
starting a poller, switching truth-source mode, or enabling the file-backed
store.

Use `warn` for recoverable but unexpected integration failures, such as a
poll-refresh plan that could not be executed, a failed background finalize
callback, or a rejected provider response where the client gets a safe response.

Use `error` only at an application boundary when the request or background job
is failing and the error has already been scrubbed of secrets.

Inside this crate, the correct level is normally "none" because the return value
already contains the decision.

## What Not To Log

The domain types carry user and key metadata. Treat them as sensitive even when
they look like friendly names.

```rust
// crates/aether-video-tasks-core/src/types.rs:123
pub struct LocalVideoTaskPersistence {
    pub request_id: String,
    pub username: Option<String>,
    pub api_key_name: Option<String>,
    pub client_api_format: String,
    pub provider_api_format: String,
    pub original_request_body: Value,
    pub format_converted: bool,
}
```

```rust
// crates/aether-video-tasks-core/src/types.rs:92
pub struct LocalVideoTaskTransport {
    pub upstream_base_url: String,
    pub provider_name: Option<String>,
    pub provider_id: String,
    pub endpoint_id: String,
    pub key_id: String,
    pub headers: BTreeMap<String, String>,
```

Never log:

- `LocalVideoTaskTransport.headers`, because it can include authorization.
- `LocalVideoTaskPersistence.original_request_body`, because prompts may contain
  user data.
- `username` or `api_key_name`, because friendly labels can still identify a
  user or credential.
- Full provider payloads or generated video URLs unless the caller explicitly
  redacts them.

## DON'T Patterns

DON'T add logging just before returning `None`:

```rust
// DON'T
tracing::warn!(request_path, "unsupported video task path");
return None;
```

Unsupported path shapes are routine in this crate. Let the caller decide whether
the miss is interesting.

DON'T instrument helper functions that parse JSON values:

```rust
// crates/aether-video-tasks-core/src/body.rs:34
pub fn request_body_string(body: &Value, key: &str) -> Option<String> {
    body.as_object()
        .and_then(|map| map.get(key))
```

These helpers may touch prompt fields and should stay pure.

DON'T log persistence file contents. If the file store fails to load, return the
`std::io::Error` from `with_file_store`; if a later best-effort mutation fails,
surface that through the existing `bool` result or change the trait contract.

## Adding Logging In The Future

If a future change genuinely requires logging inside this crate, keep it narrow:

- Add `tracing` only after deciding why return values are not enough.
- Use structured fields, not formatted blobs.
- Include `request_id` or `trace_id` when available.
- Do not include headers, raw bodies, usernames, API key names, or provider
  response bodies.
- Add tests around the behavior that caused logging to be needed; do not rely on
  logs as the only evidence.
