# Logging Guidelines

> Observability conventions for `crates/aether-model-fetch`.

---

## Scope

This crate currently has no direct `tracing`, `log`, `println!`, or `eprintln!`
usage. Repository inspection found no logging macros in
`crates/aether-model-fetch`. That is intentional: the crate returns structured
outcomes and error strings, while gateway callers log with request context.

Do not add logging here by default. Most model-fetch functions do not know the
gateway phase, admin request, provider display context, or whether a failure
will be persisted, retried, or shown as a fallback. Logging at this layer would
likely duplicate caller logs and risk exposing credential-adjacent data.

The crate's observability surface is data:

```rust
// crates/aether-model-fetch/src/strategy.rs:35
pub struct ModelsFetchOutcome {
    pub fetched_model_ids: Vec<String>,
    pub cached_models: Vec<Value>,
    pub errors: Vec<String>,
    pub has_success: bool,
    pub upstream_metadata: Option<Value>,
}
```

And scheduled callers summarize work with `ModelFetchRunSummary`:

```rust
// crates/aether-model-fetch/src/logic.rs:19
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFetchRunSummary {
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
}
```

---

## Caller Logging Pattern

The gateway worker is the correct place to log model-fetch lifecycle events.
It has the phase (`startup` or `tick`) and the run summary.

```rust
// apps/aether-gateway/src/model_fetch/runtime.rs:195
async fn run_model_fetch_cycle<S>(state: &S, phase: &'static str) -> Result<(), GatewayError>
where
    S: ModelFetchRuntimeState + ?Sized,
{
    let summary = perform_model_fetch_once_with_state(state).await?;
    if summary.attempted == 0 {
        debug!(phase, "gateway model fetch found no eligible keys");
        return Ok(());
    }

    info!(
        phase,
        attempted = summary.attempted,
        succeeded = summary.succeeded,
        failed = summary.failed,
        skipped = summary.skipped,
        "gateway model fetch cycle completed"
    );
```

Gateway startup and tick errors are warnings because the service can keep
running and retry on the next cycle.

```rust
// apps/aether-gateway/src/model_fetch/runtime.rs:41
if let Err(err) = run_model_fetch_cycle(&state, "startup").await {
    warn!(error = ?err, "gateway model fetch startup failed");
}
...
if let Err(err) = run_model_fetch_cycle(&state, "tick").await {
    warn!(error = ?err, "gateway model fetch tick failed");
}
```

Per-key failures include provider/key identifiers and sanitized messages. The
crate should return messages that make these caller logs useful.

```rust
// apps/aether-gateway/src/model_fetch/runtime.rs:316
warn!(
    provider_id = %target.provider.id,
    key_id = %target.key.id,
    message = %error,
    "gateway model fetch failed"
);
```

Cache write failures are `debug` because they should not make model fetch fail:

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

---

## What This Crate Should Return For Logs

Return provider-prefixed errors when a provider-specific path fails. Existing
messages include `antigravity:`, `vertex_ai(api_key):`,
`vertex_ai(service_account):`, `GeminiCLI`, and `Kiro` context.

```rust
// crates/aether-model-fetch/src/strategy.rs:456
let Some(auth_config) = auth_config else {
    return Ok(ModelsFetchOutcome {
        fetched_model_ids: Vec::new(),
        cached_models: Vec::new(),
        errors: vec!["vertex_ai(service_account): missing auth_config".to_string()],
        has_success: false,
        upstream_metadata: None,
    });
};
```

Return `errors` for non-fatal failures and `has_success` for the caller's log
level decisions. Do not log every fallback attempt from inside the crate.

```rust
// crates/aether-model-fetch/src/strategy.rs:430
let deduped = dedupe_models_by_id_and_format(all_models);
if !deduped.is_empty() {
    return Ok(build_success_outcome(deduped, None, true).with_errors(hard_errors));
}
```

Use `execution_result_error_message` to convert upstream failures to safe,
compact strings. It avoids dumping headers or auth config.

```rust
// crates/aether-model-fetch/src/strategy.rs:703
fn execution_result_error_message(result: &ExecutionResult) -> String {
    result
        .body
        .as_ref()
        .and_then(|body| body.json_body.as_ref())
        .and_then(extract_error_message)
        .or_else(|| {
            result.error.as_ref().and_then(|error| {
                let message = error.message.trim();
                (!message.is_empty()).then_some(message.to_string())
            })
        })
        .unwrap_or_else(|| format!("HTTP {}: upstream request failed", result.status_code))
}
```

---

## Log Levels If Logging Is Added

Prefer keeping logs in callers. If a future change has a strong reason to log
inside this crate, follow the gateway's `tracing` style and keep fields
structured.

Use `debug` for local, non-fatal decisions that are useful only during
diagnostics: provider strategy selected, page loop termination reason, fallback
base URL attempted. Avoid high-cardinality fields unless they are essential.

Use `info` only for one-per-cycle summary logs. Because this crate does not own
the cycle, `info` almost always belongs in `apps/aether-gateway`.

Use `warn` for recoverable provider failures only at the boundary where provider
ID and key ID are available. The current gateway code logs failed model fetches
at `warn`.

Use `error` rarely. A provider outage, missing credential, or unsupported format
is usually a `warn` plus persisted failure state, not a process-level error.

---

## Structured Fields

Caller logs should use explicit fields rather than formatting everything into
the message. Existing gateway logs use:

```rust
// apps/aether-gateway/src/model_fetch/runtime.rs:299
warn!(
    provider_id = %target.provider.id,
    key_id = %target.key.id,
    message = %err,
    "gateway model fetch failed"
);
```

Recommended fields for model-fetch logs are:

- `phase` for scheduled lifecycle (`startup`, `tick`, or an admin-triggered label).
- `provider_id`, `endpoint_id`, and `key_id` for target identity.
- `attempted`, `succeeded`, `failed`, and `skipped` for cycle summaries.
- `message` or `error` for sanitized provider-facing failures.

Do not log raw `ExecutionPlan.headers`, `GatewayProviderTransportKey`, decrypted
API keys, OAuth access tokens, service-account private keys, or the full
`decrypted_auth_config` JSON. Many model-fetch code paths construct credentials
or protected headers:

```rust
// crates/aether-model-fetch/src/transport.rs:573
fn insert_non_empty_auth_header(
    headers: &mut BTreeMap<String, String>,
    protected_headers: &mut Vec<String>,
    name: &str,
    value: &str,
) {
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        return;
    }

    protected_headers.push(name.to_string());
    headers.insert(name.to_string(), value.to_string());
}
```

`protected_headers` exists to stop endpoint rules from overwriting auth headers;
it is also a reminder that those header values are sensitive.

---

## Do Not

Do not add `println!`, `eprintln!`, or `dbg!` for provider troubleshooting. Use
structured `tracing` in callers or return richer sanitized errors from this
crate.

Do not log provider response bodies wholesale. Extract a message with
`extract_error_message` and let callers decide whether to persist or display it.

Do not log model lists at `info` or `warn`. Model IDs can be user-configured or
provider-specific and may be large; tests should assert them instead.

Do not log secrets from `transport.key.decrypted_api_key`,
`transport.key.decrypted_auth_config`, generated JWT assertions, bearer tokens,
or query-string API keys.

Do not add logging as a substitute for returning partial outcomes. Callers need
`errors`, `has_success`, and `upstream_metadata` to update state and cache.
