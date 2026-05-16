# Error Handling

> Error conventions for `crates/aether-model-fetch`.

---

## Scope

This crate does not define a `thiserror` enum or return gateway-specific
`GatewayError`. The crate boundary is deliberately string-based for transport
and parsing failures, and trait-associated for storage failures.

Use `Result<T, String>` for model-fetch parsing, plan construction, and
strategy execution. Use a trait-associated error only when the caller owns the
repository implementation.

```rust
// crates/aether-model-fetch/src/strategy.rs:77
pub async fn fetch_models_from_transports(
    runtime: &(impl ModelFetchTransportRuntime + ?Sized),
    transports: &[GatewayProviderTransportSnapshot],
) -> Result<ModelsFetchOutcome, String> {
    let strategy = select_model_fetch_strategy(transports)?;
    execute_model_fetch_strategy(runtime, transports, strategy).await
}
```

Callers are responsible for mapping these crate-level strings into their own
error type. The gateway runtime maps association sync failures with
`GatewayError::Internal` and persists upstream fetch failures on the provider
catalog key.

---

## Public Error Shapes

`Result<T, String>` is used where the crate can produce a concrete domain
message without knowing the caller's error type. Examples include response
parsing:

```rust
// crates/aether-model-fetch/src/logic.rs:93
pub fn parse_models_response_page(
    endpoint_api_format: &str,
    body: &Value,
) -> Result<ModelsFetchPage, String> {
    let api_format = normalize_api_format(endpoint_api_format);
    ...
    } else {
        return Err("models response parser does not support this provider format".to_string());
    }
}
```

`ModelsFetchOutcome` carries partial-success state. This is not an error return:
it allows one endpoint or region to fail while another produces usable models.

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

`ModelFetchAssociationStore` uses an associated error because association sync
delegates storage to the caller. Keep this pattern when adding storage-like
operations.

```rust
// crates/aether-model-fetch/src/association_sync.rs:15
#[async_trait]
pub trait ModelFetchAssociationStore {
    type Error: Send;

    fn has_global_model_reader(&self) -> bool;
    fn has_global_model_writer(&self) -> bool;
    fn model_fetch_internal_error(&self, message: String) -> Self::Error;
```

The gateway implementation chooses `String` for this associated error and maps
gateway repository errors with `format!("{err:?}")`.

```rust
// apps/aether-gateway/src/state/integrations.rs:188
#[async_trait]
impl ModelFetchAssociationStore for AppState {
    type Error = String;

    fn model_fetch_internal_error(&self, message: String) -> Self::Error {
        message
    }
```

---

## Propagation Patterns

Prefer `ok_or_else` with precise domain messages for required provider fields.
Do not return generic "invalid input" messages when the missing field is known.

```rust
// crates/aether-model-fetch/src/strategy.rs:254
let project_id = auth_config
    .as_ref()
    .and_then(|value| value.get("project_id"))
    .and_then(Value::as_str)
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| "antigravity: missing auth_config.project_id (please re-auth)".to_string())?
    .to_string();
```

Use `?` to keep fallible plan construction and runtime execution linear. This
crate favors readable fail-fast chains over nested `match` blocks unless the
code needs to classify a soft failure.

```rust
// crates/aether-model-fetch/src/strategy.rs:210
let plan = build_standard_models_fetch_execution_plan(
    runtime,
    transport,
    next_after_id.as_deref(),
)
.await?;
let result = runtime.execute_model_fetch_execution_plan(&plan).await?;
let body_json = execution_result_json_body(&result)?;
let parsed = parse_models_response_page(&transport.endpoint.api_format, &body_json)?;
```

Use an early `Ok(empty outcome with errors)` when the provider/key is eligible
for model fetch but lacks credentials. This lets the gateway persist the failure
without treating it as an internal code error.

```rust
// crates/aether-model-fetch/src/strategy.rs:390
if api_key.is_empty() || api_key == "__placeholder__" {
    return Ok(ModelsFetchOutcome {
        fetched_model_ids: Vec::new(),
        cached_models: Vec::new(),
        errors: vec!["vertex_ai(api_key): missing api key".to_string()],
        has_success: false,
        upstream_metadata: None,
    });
}
```

When provider alternatives are expected, collect errors and continue. For
Antigravity, HTTP 404/408/429/5xx can fall through to the next base URL.

```rust
// crates/aether-model-fetch/src/strategy.rs:282
let result = match runtime.execute_model_fetch_execution_plan(&plan).await {
    Ok(result) => result,
    Err(err) => {
        errors.push(format!("{base_url}: {err}"));
        continue;
    }
};
```

```rust
// crates/aether-model-fetch/src/strategy.rs:883
fn should_fallback_antigravity_status(status_code: u16) -> bool {
    matches!(status_code, 404 | 408 | 429) || (500..600).contains(&status_code)
}
```

---

## Upstream Error Extraction

Always prefer upstream JSON error messages before falling back to raw execution
errors or HTTP status text. This preserves provider diagnostics while keeping a
stable fallback.

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

`extract_error_message` supports both OpenAI-like `{ "error": { "message": ... } }`
and simpler `{ "message": ... }` payloads:

```rust
// crates/aether-model-fetch/src/logic.rs:41
pub fn extract_error_message(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
```

---

## Caller Surface

This crate does not format API responses. It returns outcomes and strings; the
gateway decides whether to persist, log, or return them. The main scheduled path
persists failures on the key and logs at the gateway boundary:

```rust
// apps/aether-gateway/src/model_fetch/runtime.rs:295
let result = match fetch_models_from_transports(state, &transports).await {
    Ok(result) => result,
    Err(err) => {
        persist_key_fetch_failure(state, &target.key, now_unix_secs, err.clone()).await?;
        warn!(
            provider_id = %target.provider.id,
            key_id = %target.key.id,
            message = %err,
            "gateway model fetch failed"
        );
        return Ok(KeyFetchDisposition::Failed);
    }
};
```

Admin query handlers can use the same `fetch_models_from_transports` entry point
and decide on fallback behavior:

```rust
// apps/aether-gateway/src/handlers/admin/provider/query/models.rs:1707
let outcome = match fetch_models_from_transports(state.app(), &transports).await {
    Ok(outcome) => outcome,
    Err(err) => {
        all_errors.push(err);
        if let Some(fallback) = provider_query_codex_preset_fallback(provider) {
            return Ok(fallback);
        }
```

---

## Do Not

Do not introduce `anyhow::Error` at this crate boundary. It would hide the stable
domain strings that gateway code stores in `last_models_fetch_error`.

Do not panic in runtime code. `expect(...)` is acceptable in tests that build
fixtures, such as `StoredProviderCatalogEndpoint::new(...).expect("endpoint should build")`
in `crates/aether-model-fetch/src/logic.rs:627`.

Do not discard partial success. If one region or endpoint succeeds, return
`has_success: true` and carry non-fatal messages in `errors`.

Do not log and return the same error inside this crate. Return the message or
outcome; logging belongs to the caller that has provider ID, key ID, phase, and
request context.

Do not convert provider credential problems into internal errors. Prefer a
provider-prefixed message like `vertex_ai(service_account): missing project_id`
or `Kiro models fetch requires Kiro request auth`.
