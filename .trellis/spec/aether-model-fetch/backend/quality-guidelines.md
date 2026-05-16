# Quality Guidelines

> Code quality standards for `crates/aether-model-fetch`.

---

## Scope

Quality in this crate means keeping provider model-fetch behavior deterministic,
testable without real upstream calls, and isolated from gateway storage/runtime
implementations. New code should preserve the split between pure logic,
transport plan construction, strategy execution, and caller-provided storage.

The crate is not a generic Rust utility library. Every public type or function
should relate directly to provider model discovery, cached model normalization,
key whitelist calculation, or provider/global model association sync.

---

## Required Patterns

Keep modules private and re-export only the stable public API from `lib.rs`.

```rust
// crates/aether-model-fetch/src/lib.rs:13
pub use logic::{
    aggregate_models_for_cache, apply_model_filters, build_models_fetch_url,
    endpoint_supports_rust_models_fetch, extract_error_message, json_string_list,
    merge_upstream_metadata, parse_models_response, parse_models_response_page,
    preset_models_for_provider, provider_type_uses_preset_models, select_models_fetch_endpoint,
    selected_models_fetch_endpoints, ModelFetchRunSummary, ModelsFetchPage, ModelsFetchSuccess,
};
```

Normalize external strings at the boundary. Existing code trims provider types,
auth types, API formats, model IDs, and environment variables before matching.

```rust
// crates/aether-model-fetch/src/strategy.rs:92
let provider_type = first_transport
    .provider
    .provider_type
    .trim()
    .to_ascii_lowercase();
```

Prefer deterministic collections for output order and duplicate removal.
`BTreeSet` and `BTreeMap` are used throughout parsing and aggregation so tests
and cache contents remain stable.

```rust
// crates/aether-model-fetch/src/logic.rs:377
pub fn aggregate_models_for_cache(models: &[Value]) -> Vec<Value> {
    let mut aggregated = BTreeMap::<String, serde_json::Map<String, Value>>::new();

    for model in models {
        let Some(object) = model.as_object() else {
            continue;
        };
```

Preserve API format information as an array. Legacy single `api_format` fields
are normalized into `api_formats` and removed from cached records.

```rust
// crates/aether-model-fetch/src/logic.rs:431
let mut merged_formats = existing_formats
    .union(&api_formats)
    .cloned()
    .collect::<BTreeSet<_>>();
if let Some(api_format) = legacy_api_format {
    merged_formats.insert(api_format);
}
let merged_formats = merged_formats
    .into_iter()
    .map(Value::String)
    .collect::<Vec<_>>();
entry.insert("api_formats".to_string(), Value::Array(merged_formats));
```

Build transport requests as `ExecutionPlan` values, not direct network calls.
This keeps model-fetch behavior testable and lets the gateway runtime apply the
same execution/proxy/timeout path as normal provider traffic.

```rust
// crates/aether-model-fetch/src/transport.rs:354
Ok(ExecutionPlan {
    request_id: format!(
        "req-model-fetch-{}-{}",
        transport.key.id,
        provider_api_format.replace(':', "-")
    ),
    candidate_id: None,
    provider_name: Some(transport.provider.name.clone()),
    provider_id: transport.provider.id.clone(),
```

Use trait abstractions where the crate needs caller-owned behavior. The runtime
trait keeps OAuth resolution, proxy lookup, and execution outside the crate.

```rust
// crates/aether-model-fetch/src/transport.rs:62
async fn execute_model_fetch_execution_plan(
    &self,
    plan: &ExecutionPlan,
) -> Result<ExecutionResult, String>;
```

For provider strategy branching, add a `ModelFetchStrategyKind` variant and keep
selection centralized. Do not sprinkle provider-specific checks across callers.

```rust
// crates/aether-model-fetch/src/strategy.rs:134
let kind = match provider_type.as_str() {
    "antigravity" => ModelFetchStrategyKind::Antigravity,
    "vertex_ai" => ModelFetchStrategyKind::Vertex,
    _ => ModelFetchStrategyKind::StandardTransport,
};
```

---

## Provider Data Rules

Model IDs must be trimmed and non-empty before entering output vectors. OpenAI,
Claude, Gemini, Vertex, Kiro, and Antigravity all have slightly different field
names; parsing helpers must normalize them before returning cached models.

```rust
// crates/aether-model-fetch/src/logic.rs:525
fn model_id_from_openai_like_item(item: &Value) -> Option<String> {
    if let Some(value) = item
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(value.trim_start_matches("models/").to_string());
    }
```

Filter behavior must preserve locked models even if they do not match include
patterns. This is intentional: locked models are administrator overrides.

```rust
// crates/aether-model-fetch/src/logic.rs:353
for model in locked_models {
    let trimmed = model.trim();
    if !trimmed.is_empty() {
        filtered.insert(trimmed.to_string());
    }
}
```

Provider metadata merges must preserve only live upstream keys and carry forward
reset time for returned quota entries. `merge_upstream_metadata` intentionally
drops stale models when they are missing from the new upstream payload.

```rust
// crates/aether-model-fetch/src/logic.rs:303
for (model_id, new_info) in new_quota.iter_mut() {
    let Some(new_info_object) = new_info.as_object_mut() else {
        continue;
    };
    let Some(old_info_object) = old_quota.get(model_id).and_then(Value::as_object)
    else {
        continue;
    };
    if !new_info_object.contains_key("reset_time") {
```

---

## Testing Requirements

Place tests in the same module as the behavior under test. This crate uses
module-local `#[cfg(test)] mod tests` blocks in `config.rs`, `logic.rs`,
`transport.rs`, and `strategy.rs`.

Use plain `#[test]` for pure helpers and `#[tokio::test]` for async strategy or
plan construction. Do not call real provider endpoints in tests; use a fake
runtime implementing `ModelFetchTransportRuntime`.

```rust
// crates/aether-model-fetch/src/strategy.rs:1298
struct TestRuntime {
    executed_urls: Arc<Mutex<Vec<String>>>,
    response_body: Value,
}

#[async_trait]
impl ModelFetchTransportRuntime for TestRuntime {
    async fn resolve_local_oauth_request_auth(
        &self,
        _transport: &GatewayProviderTransportSnapshot,
    ) -> Result<Option<aether_provider_transport::LocalResolvedOAuthRequestAuth>, String>
```

Plan-construction tests should assert URLs, provider API formats, headers, and
auth behavior. Example coverage already exists for OpenAI Responses, Codex,
Claude pagination, Gemini query auth, Antigravity, Gemini CLI, Kiro, and Vertex.

```rust
// crates/aether-model-fetch/src/transport.rs:741
#[tokio::test]
async fn builds_codex_models_fetch_plan_with_account_header() {
    let runtime = TestRuntime {
        oauth_auth: Some(
            aether_provider_transport::LocalResolvedOAuthRequestAuth::Header {
                name: "authorization".to_string(),
                value: "Bearer access-token".to_string(),
            },
        ),
        proxy: None,
    };
```

Parser tests should assert deduplication and normalized `api_formats`.

```rust
// crates/aether-model-fetch/src/logic.rs:696
fn aggregate_models_for_cache_merges_api_formats_and_sorts_by_model_id() {
    let aggregated = aggregate_models_for_cache(&[
        json!({"id":"zeta","api_formats":["openai:chat"]}),
        json!({"id":"alpha","api_formats":["openai:responses"]}),
        json!({"id":"alpha","api_formats":["openai:chat"]}),
    ]);
```

When adding a provider, include at least one strategy-selection test and one
end-to-end fake-runtime test that verifies the resulting `ModelsFetchOutcome`.

---

## Forbidden Patterns

Do not add direct HTTP clients such as `reqwest` to this crate. Build an
`ExecutionPlan` and execute it through `ModelFetchTransportRuntime`.

Do not add concrete gateway imports, SeaORM entities, Redis clients, or axum
extractors. The correct caller boundary is shown by `ModelFetchRuntimeState`,
which composes this crate's traits with gateway-specific methods outside the
crate:

```rust
// apps/aether-gateway/src/model_fetch/runtime/state.rs:16
#[async_trait]
pub(crate) trait ModelFetchRuntimeState:
    ModelFetchAssociationStore<Error = String> + ModelFetchTransportRuntime + Sync
{
    fn has_provider_catalog_data_reader(&self) -> bool;
    fn has_provider_catalog_data_writer(&self) -> bool;
```

Do not use unordered maps for returned model lists or cache aggregation when
order affects tests, JSON cache shape, or UI output. Prefer `BTreeMap` and
`BTreeSet` unless order is irrelevant and private.

Do not silently accept unsupported API formats. Unsupported formats should
return `None` for URL selection or a clear `Err(String)` from parsers.

```rust
// crates/aether-model-fetch/src/logic.rs:66
if !endpoint_supports_rust_models_fetch(&api_format) {
    return None;
}
```

Do not add broad re-exports just because a test needs a helper. Prefer testing
private helpers from the same module's test block.

Do not log secrets, headers, decrypted API keys, OAuth tokens, service-account
private keys, or full `auth_config` JSON. The crate currently avoids logging
entirely; callers log provider/key IDs and sanitized error strings.

---

## Review Checklist

Confirm new provider behavior is placed in the right module: URL and headers in
`transport.rs`, selection and pagination in `strategy.rs`, response normalization
in `logic.rs`, and storage association behavior in `association_sync.rs`.

Confirm public APIs are re-exported from `lib.rs` only when needed by a caller.

Confirm every external string is trimmed and normalized before comparison.

Confirm duplicate models are handled deterministically and preserve all relevant
API formats.

Confirm partial success semantics are preserved: endpoint or region errors go
into `ModelsFetchOutcome.errors` when another path can still succeed.

Confirm tests use fake runtimes and static JSON payloads, not real network calls.

Confirm `cargo test -p aether-model-fetch` passes before changing adjacent
gateway behavior.
