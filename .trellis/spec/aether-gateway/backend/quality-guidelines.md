# Quality Guidelines

`apps/aether-gateway` favors narrow visibility, explicit route taxonomy,
facade-based data access, structured responses, and behavior tests around
route/execution contracts. The crate is large and currently carries broad lint
allows, so new work should avoid expanding loose patterns even when legacy code
contains them.

## Visibility

Default modules are private. Promote only what is needed by dependent crates or
tests.

```rust
// apps/aether-gateway/src/lib.rs:49
pub(crate) mod middleware;
pub(crate) use self::error::GatewayError;
pub use self::router::{attach_static_frontend, build_router, build_router_with_state, serve_tcp};
pub use self::state::{AppState, FrontdoorCorsConfig};
```

Use `pub(crate)` for cross-module gateway internals and plain private items for
file-local helpers. Avoid `pub` on handler, control, executor, or data helpers
unless external crates need them.

## Type-Safe Runtime Outcomes

Execution paths return enums instead of loosely typed status strings. Preserve
this shape when adding new execution paths.

```rust
// apps/aether-gateway/src/executor/outcome.rs:28
#[derive(Debug)]
pub(crate) enum LocalExecutionRequestOutcome {
    Responded(Response<Body>),
    Exhausted(LocalExecutionExhaustion),
    NoPath,
}
```

Route decisions are also typed as a struct, with route taxonomy carried as
fields that later become response headers and logs.

```rust
// apps/aether-gateway/src/control/route/mod.rs:15
#[derive(Debug, Clone)]
pub(crate) struct GatewayControlDecision {
    pub(crate) public_path: String,
    pub(crate) public_query_string: Option<String>,
    pub(crate) route_class: Option<String>,
    pub(crate) route_family: Option<String>,
    pub(crate) route_kind: Option<String>,
    pub(crate) request_auth_channel: Option<String>,
    pub(crate) auth_endpoint_signature: Option<String>,
    pub(crate) execution_runtime_candidate: bool,
```

When adding a route or execution step, update the typed route/execution flow
instead of passing ad-hoc booleans through unrelated modules.

## Boundary Adapters

The gateway adapts lower-layer crates through narrow ports. The executor maps
`aether_ai_serving` outcomes into gateway outcomes rather than exposing serving
crate types everywhere.

```rust
// apps/aether-gateway/src/executor/stream_path.rs:83
impl AiStreamExecutionPathPort for GatewayStreamExecutionPathPort<'_> {
    type Response = Response<Body>;
    type Exhaustion = super::LocalExecutionExhaustion;
    type Error = GatewayError;

    fn scheduler_decision_supported(&self) -> bool {
        self.scheduler_supported
    }
```

Keep this adapter style for new cross-crate logic. Define a gateway port/adapter
when a lower-level crate needs `AppState`, `GatewayError`, `Response<Body>`, or
trace-aware behavior.

## Response Finalization

Every frontdoor response should pass through finalization. This attaches control
headers, trace id, execution path, audit events, access logs, and any held
request permit.

```rust
// apps/aether-gateway/src/handlers/proxy/finalize.rs:43
pub(super) fn finalize_gateway_response(
    state: &AppState,
    mut response: Response<Body>,
    trace_id: &str,
    remote_addr: &std::net::SocketAddr,
    method: &http::Method,
    path_and_query: &str,
    control_decision: Option<&GatewayControlDecision>,
    execution_path: &'static str,
    started_at: &Instant,
    request_permit: Option<AdmissionPermit>,
) -> Response<Body> {
```

Do not return raw local responses from `proxy_request` after route/context
resolution. Use `finalize_gateway_response_with_context`.

## Header Handling

Use shared helpers when adding headers. `insert_header_if_missing` returns
`GatewayError` on invalid values and avoids overwriting upstream/header metadata.

```rust
// apps/aether-gateway/src/lib.rs:109
fn insert_header_if_missing(
    headers: &mut http::HeaderMap,
    key: &'static str,
    value: &str,
) -> Result<(), GatewayError> {
    if headers.contains_key(key) {
        return Ok(());
    }
    let name = HeaderName::from_static(key);
    let value =
        HeaderValue::from_str(value).map_err(|err| GatewayError::Internal(err.to_string()))?;
    headers.insert(name, value);
    Ok(())
}
```

Do not duplicate skip lists or sensitive-header logic. Use existing helpers in
`headers.rs` and `handlers/shared`.

## Testing Requirements

Route classification changes require route tests under `src/control/tests`.
These tests are fast and lock the route taxonomy contract.

```rust
// apps/aether-gateway/src/control/tests/admin_usage.rs:5
#[test]
fn classifies_admin_usage_stats_as_admin_proxy_route() {
    let uri: Uri = "/api/admin/usage/stats".parse().expect("uri should parse");
    let headers = http::HeaderMap::new();
    let decision =
        classify_control_route(&http::Method::GET, &uri, &headers).expect("route should classify");
    assert_eq!(decision.route_class.as_deref(), Some("admin_proxy"));
    assert_eq!(decision.route_family.as_deref(), Some("usage_manage"));
    assert_eq!(decision.route_kind.as_deref(), Some("stats"));
```

Middleware and HTTP response behavior should be tested with axum `Router` and
`tower::ServiceExt::oneshot` when possible.

```rust
// apps/aether-gateway/src/middleware/access_log.rs:216
#[tokio::test(flavor = "current_thread")]
async fn access_log_emits_completed_events_by_default() {
    let writer = SharedBuffer::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .json()
            .with_writer(writer.clone())
```

State-backed behavior often uses `AppState::new()` plus `with_*_for_tests`
helpers instead of real databases. For example, `AppState` contains optional
test stores behind `#[cfg(test)]`.

```rust
// apps/aether-gateway/src/state/app.rs:76
#[cfg(test)]
pub(crate) provider_oauth_state_store: Option<Arc<StdMutex<HashMap<String, String>>>>,
#[cfg(test)]
pub(crate) auth_user_store: Option<
    Arc<StdMutex<HashMap<String, aether_data::repository::users::StoredUserAuthRecord>>>,
>,
```

Use real data-layer/integration tests only when the behavior depends on
repository implementation, migrations, or SQL semantics.

## Scenario: Admin provider-query model tests simulate one provider/pool request

### 1. Scope / Trigger
- Trigger: changes that touch `/api/admin/provider-query/test-model`,
  `/api/admin/provider-query/test-model-failover`, endpoint/provider-type
  adapters, or pool-key candidate ordering.
- This is a cross-layer contract because the admin catalog, transport snapshot, and gateway test handlers must agree on what a model test is.

### 2. Signatures
- `provider_query_test_mode(payload: &Value) -> &str`
- `provider_query_should_apply_model_mapping(payload: &Value) -> bool`
- `provider_query_extract_mapped_model_name(payload: &Value) -> Option<String>`
- `provider_query_resolve_global_effective_model(state, provider_id, requested_model, endpoint) -> Result<String, GatewayError>`
- `provider_query_mapped_model_matches_selected_endpoint(...) -> Result<bool, GatewayError>`
- `provider_query_test_adapter_for_provider_api_format(...) -> Option<ProviderQueryTestAdapter>`
- `provider_query_model_test_endpoint_priority(...) -> Option<u8>`
- `provider_query_resolve_standard_test_upstream_is_stream(endpoint_config, provider_type, provider_api_format) -> bool`
- `provider_query_request_requires_body_stream_field(request_body, endpoint_config) -> bool`
- `provider_query_standard_test_unsupported_reason(...) -> String`
- `provider_query_execute_standard_test_candidate(...) -> Result<ProviderQueryExecutionOutcome, GatewayError>`
- `provider_query_execute_openai_image_test_candidate(...) -> Result<ProviderQueryExecutionOutcome, GatewayError>`
- `provider_query_execute_antigravity_test_candidate(...) -> Result<ProviderQueryExecutionOutcome, GatewayError>`
- `provider_query_candidate_summary_payload(total_candidates, total_attempts, attempts) -> Value`
- `build_admin_provider_query_kiro_failover_response(...) -> Result<Response<Body>, GatewayError>`
- `build_admin_provider_query_test_model_local_response(...) -> Result<Response<Body>, GatewayError>`
- `build_admin_provider_query_test_model_failover_local_response(...) -> Result<Response<Body>, GatewayError>`

### 3. Contracts
- A model test simulates one real user request against the selected provider or
  provider pool. Do not add a separate admin-only "global route" test path.
- `mode = global` and `mode = pool` may apply the selected provider model's
  `provider_model_mappings` before building the upstream request. The model
  list test request model is the user's selected/global model; the execution
  model is either the selected model as-is or the endpoint-scoped mapped provider
  model chosen by the user.
- When the UI exposes mapped models, it must derive the dropdown options from
  mappings that match the selected endpoint's API format and endpoint scope.
  Changing the endpoint must recompute the options and synchronize the request
  body `model` with the selected model option. The model-list UI defaults to
  "use current model" even when mappings exist; selecting a mapped model from
  the dropdown is the explicit action that updates the request body. If the
  selected endpoint has no matching mappings, do not show a model-mapping
  control.
- A client may send `mapped_model_name` to choose one concrete mapped model from
  the endpoint-scoped options. The backend must validate that explicit choice
  against the selected model and endpoint before building the upstream request.
  A UI/client may still send `apply_model_mapping = false` for compatibility or
  explicit as-is tests, but the normal model-list UI should prefer the mapped
  model dropdown instead of a boolean switch.
- Kiro model tests follow the same explicit mapping execution rule as other
  providers. Kiro upstream model ids use dotted names such as
  `claude-opus-4.6`, `claude-haiku-4.5`, and `claude-sonnet-4.6`; when the UI
  sends an endpoint-scoped `mapped_model_name`, the backend must validate that
  selected mapping and send it as
  `conversationState.currentMessage.userInputMessage.modelId`.
- `mode = direct` bypasses global model-name resolution only. It must not bypass
  provider/pool candidate selection, endpoint capability checks, or transport
  execution. Direct mapping tests may intentionally send the mapping name as-is.
- Provider type and pool scheduling are independent axes: provider type chooses
  the endpoint adapter; pool scheduling chooses key/candidate order.
- `mode = pool` for a provider with `pool_advanced` must route candidate
  ordering and scheduler-level skips through the same pool scheduler primitive
  used by real gateway execution. Do not sort pool test keys with an
  admin-only health/priority tuple after the provider has declared pool
  scheduling presets.
- Pool tests must stop after the first successful candidate, matching user
  request failover behavior. They must not continue calling every account after
  success.
- Endpoint adapters must cover standard text formats, synchronous embedding and
  rerank formats (`openai:embedding`, `gemini:embedding`, `jina:embedding`,
  `doubao:embedding`, `openai:rerank`, `jina:rerank`), Kiro, OpenAI image
  endpoints (`codex` / `chatgpt_web`), Gemini CLI, and Antigravity endpoint-test
  envelopes when their transport snapshot declares those formats.
- Task/resource endpoints such as `openai:video`, `gemini:video`, and
  `gemini:files` are not model-test endpoints. The admin model-test UI should
  not offer them as selectable endpoints; they need separate task/file endpoint
  diagnostics if tested later.
- Keep `handlers/admin/provider/query/models/` split by responsibility:
  model listing/cache in `mod.rs`, test adapter support in
  `model_test/adapter.rs`, model mapping resolution in
  `model_test/model_mapping.rs`, and result summaries in
  `model_test/summary.rs`. Do not put new endpoint-specific test logic back
  into a single monolithic `models.rs`.
- Embedding and rerank model tests are data API tests, not chat tests. They
  should use the OpenAI-shaped admin request payload as the client shape,
  convert through the standard format registry for provider-specific formats,
  omit `stream` from provider bodies, and keep real transport URL/auth/header
  behavior.
- `doubao:embedding` currently means the Doubao text embedding API only:
  provider URL `/api/v3/embeddings` and provider body
  `{"model": "...", "input": ["..."]}`. Do not route it to
  `/api/v3/embeddings/multimodal` or emit `{"type":"text","text":"..."}` input
  objects unless a separate multimodal format/variant is introduced.
- `gemini:embedding` single-input model tests route to
  `/v1beta/models/{model}:embedContent` and emit `content.parts[].text`.
  Batch `requests[]` requires a separate batch endpoint decision; do not infer
  batch support by sending `requests[]` to `:embedContent`.
- Standard text model tests must preserve the same upstream streaming policy as
  real gateway execution. In particular, `provider_type = codex` with
  `openai:responses` requires upstream streaming even when the admin model-test
  UI is a synchronous diagnostic flow. Do not force the admin test execution
  plan or request body to `stream = false`.
- When a standard text model test sends a forced-stream upstream request, the
  returned SSE bytes must be aggregated back into the corresponding sync JSON
  body before populating attempt `response_body` and the top-level success
  payload. Do not treat a `200` stream response with only `body_bytes_b64` as a
  complete model-test success.
- Standard model-test candidates are successful only when the upstream status is
  successful and the gateway has produced a finalized model-test response body.
  A `2xx` result with no `json_body` and no aggregatable stream body is a failed
  attempt and must continue failover instead of stopping the candidate loop.
- Unsupported standard-test candidates must keep the original request body and surface a skipped attempt with a concrete reason.
- If no candidate succeeds, the response should prefer the last concrete `error_message` or `skip_reason` rather than a generic placeholder.
- Successful and failed model-test responses must include `provider.provider_type`
  and `candidate_summary` so the UI can render provider-type and scheduler
  semantics separately.
- Model tests that receive a `request_id` and run with a request-candidate
  writer must persist live `request_candidates` for the same id. Seed generated
  candidates as `available`, mark the active candidate `pending` before
  execution, finalize it as `success`, `failed`, or `skipped`, and mark
  candidates after the first success as `unused`. Persistence failure must be
  logged and must not turn an otherwise valid model-test response into a
  failure.
- `candidate_summary` fields are `total_candidates`, `attempted`, `success`,
  `failed`, `skipped`, `unused`, `pending`, `available`, `completed`,
  `stop_reason`, and optional winning-candidate fields. `unused` means
  "not requested because a previous candidate already succeeded", not "still
  pending".

### 4. Validation & Error Matrix
- Unsupported transport / API-format pair -> `status = skipped` with a reason such as `transport_provider_type_unsupported`.
- Missing provider or model -> bad request / not found, not a generic internal error.
- Empty non-Kiro candidate set -> a failover simulation response, not a fabricated success.
- Direct mode only changes the effective model lookup; it does not bypass transport capability checks.
- Global/pool model-list UI default -> request body `model` and execution plan
  `model_name` use the selected model as-is, even when scoped mappings exist.
- Global/pool mode with a valid explicit `mapped_model_name` -> request body
  `model` and execution plan `model_name` use that exact mapped model rather
  than auto-picking the first mapping.
- Kiro global/pool mode with a valid explicit `mapped_model_name` -> validate
  the alias, but execution plan `model_name` and Kiro envelope `modelId` use the
  selected row's `provider_model_name`.
- Global/pool mode with an explicit `mapped_model_name` that is not valid for
  the selected model and endpoint -> HTTP `400`,
  `mapped_model_name is not valid for the selected model and endpoint`.
- Global/pool mode with `apply_model_mapping = false` -> request body `model`
  and execution plan `model_name` use the selected model as-is, even when a
  scoped provider-model mapping exists.
- OpenAI image endpoint test uses the image request adapter and returns the
  concrete upstream/proxy failure if the browser/OAuth transport cannot execute.
- Codex `openai:responses` standard-test candidate -> execution plan
  `stream = true` and final upstream request body `stream = true`; otherwise
  upstreams that enforce streaming can fail with `"Stream must be set to true"`.
- Codex `openai:responses` standard-test candidate with HTTP `200` stream bytes
  -> aggregate the stream into an OpenAI Responses JSON body; if aggregation
  fails, the model-test response must still expose that missing response body
  instead of flipping success based only on the status code.
- Standard candidate with HTTP `2xx` but no finalized response body -> failed
  attempt with a concrete error message; multi-candidate tests should continue
  to the next candidate.
- Antigravity endpoint test uses `EndpointTest` envelope semantics, not the
  public-agent envelope.
- First success -> `candidate_summary.stop_reason = "first_success"` and
  remaining candidates are counted as `unused`.
- Pool scheduler skipped candidates, such as exhausted or cooldown accounts,
  should surface as `status = skipped` attempts and reduce the remaining
  `unused` count rather than being hidden by the first successful scheduled
  candidate.
- All skipped without a real upstream attempt -> `stop_reason = "all_skipped"`.
- No candidate generated -> `stop_reason = "no_candidate"`.

### 5. Good/Base/Bad Cases
- Good: a pool-managed provider tries keys in scheduler order and stops on the
  first successful model-test request.
- Good: custom-provider results render as key candidates, while pool-provider
  results render as account scheduling; both come from the same response
  contract.
- Base: direct-mode tests remain explicit for callers that want to bypass global
  model-name lookup.
- Base: no matching provider model mapping for the selected endpoint falls back
  to the provider model name / requested model.
- Base: multiple mappings for one endpoint are presented as concrete dropdown
  options ordered by mapping priority/name; the selected value is sent as
  `mapped_model_name`.
- Base: a synchronous-looking admin model test may still send a streaming
  upstream request when the selected provider endpoint requires it; the gateway
  can aggregate/finalize the result before returning the diagnostic response.
- Bad: show a successful candidate with HTTP `200` while the response-body tab is
  empty and the top-level model-test result is failed because stream bytes were
  never aggregated.
- Bad: pre-filter unsupported candidates away so the UI only sees a generic "simulation not configured" message.
- Bad: call upstream with the global/alias model from the model list when a
  scoped provider-model mapping exists.
- Bad: show a mapping control for mappings that belong to another endpoint, or
  leave the request body `model` stale after the endpoint or mapped model
  selection changes.
- Bad: treat `unused` as pending work in UI copy after a successful pool test.
- Bad: add a separate "global route" admin test that can pass while the normal
  provider/pool request path would fail.

### 6. Tests Required
- Test that non-Kiro unsupported transport returns a visible skip reason.
- Test that a legacy imported provider config can immediately pass the model-test smoke.
- Test that failover responses preserve skipped attempts and the original request body.
- Test that `candidate_summary` reports `first_success` and `unused` candidates
  when a candidate succeeds before the end of the pool.
- Test that a model test with a supplied `request_id` persists candidate trace
  rows and finalizes the winning candidate as `success` and uncalled candidates
  as `unused`.
- Test that a model-list request with `provider_model_mappings` sends the mapped
  provider model in the execution plan and request body, while keeping the
  selected model in the response.
- Test that a model-list request with multiple endpoint-scoped mappings can
  select an explicit `mapped_model_name`, sends that exact model upstream, and
  rejects a mapping that belongs to a different endpoint.
- Test that a Kiro model-list/pool request with explicit `mapped_model_name`
  validates the alias but sends `models.provider_model_name` as
  `conversationState.currentMessage.userInputMessage.modelId`.
- Test that `apply_model_mapping = false` disables provider-model mapping in
  the execution plan and request body for model-list/failover tests.
- Test that Codex `openai:responses` provider-query model tests set both
  `ExecutionPlan.stream` and upstream request-body `stream` to `true`.
- Test that a standard `openai:responses` stream body is aggregated into the
  attempt response body used by model-test result summaries.
- Test the provider-type/endpoint adapter matrix for `custom`, `kiro`, `codex`,
  `chatgpt_web`, `gemini_cli`, and `antigravity`.

### 7. Wrong vs Correct
#### Wrong
- Treat provider type and pool scheduling as one boolean branch, or keep calling
  every key after the first successful pool candidate.
- Ignore `provider_model_mappings` in model-list tests, letting upstream alias
  compatibility hide a broken Rust-side mapping.
- Let the UI request body show the alias while the backend silently sends the
  mapped model, or vice versa.
- Force standard admin model-test requests to non-streaming regardless of
  provider type or endpoint config.
- Let standard admin model tests call a streaming upstream and then read only
  `json_body`, discarding the returned `body_bytes_b64` stream payload.
- Render all model-test progress as a generic "pending/success/failure" counter
  without preserving whether the candidates are keys, accounts, or skipped
  scheduler candidates.
#### Correct
- Keep endpoint adapter choice separate from candidate ordering; execute one
  simulated user request at a time and report concrete unsupported reasons
  instead of erasing them.
- Resolve model-list tests through the same provider-model mapping selection
  rule as scheduler candidate selection when `apply_model_mapping` is enabled,
  then send the mapped model upstream.
- When a user selects a concrete mapped model in the UI, validate it against the
  selected endpoint and send that exact model upstream; keep the visible request
  body synchronized with the dropdown.
- For Kiro, treat the selected mapping as the final upstream `modelId` after
  validation; do not replace it with the provider model row's original name.
- Resolve upstream stream policy from provider type, API format, and endpoint
  config before building standard model-test requests, then re-enforce the
  request-body `stream` field when the selected upstream requires it.
- For forced-stream standard model tests, decode `body_bytes_b64` and run the
  format-specific stream aggregator (`openai:responses`, `openai:chat`,
  `claude:messages`, or `gemini:generate_content`) before setting the attempt
  `response_body`.
- Return a candidate summary from the backend and let the UI choose labels from
  `provider_type` + test mode, while keeping the execution path unified.

## Scenario: Python provider-config import format compatibility

### 1. Scope / Trigger
- Trigger: changes to `/api/admin/system/config/import`, provider endpoint
  import, provider key format import, or model-test smoke coverage after a
  system-config import.
- Python exports can contain retired CLI API-format aliases that are not the
  canonical Rust storage signatures.

### 2. Signatures
- `POST /api/admin/system/config/import`
- `normalize_import_endpoint_format(value: &str) -> Result<String, String>`
- `normalize_import_key_formats(item: &ImportedProviderKey, endpoint_formats: &[String]) -> (Vec<String>, Vec<String>)`
- `admin_endpoint_signature_parts(value: &str) -> Option<(... )>`

### 3. Contracts
- Import-time endpoint format normalization must map:
  - `openai:cli` -> `openai:responses`
  - `openai:compact` -> `openai:responses:compact`
  - `claude:chat` and `claude:cli` -> `claude:messages`
  - `gemini:chat` and `gemini:cli` -> `gemini:generate_content`
- Key `api_formats` must be normalized against the normalized endpoint
  signatures, not against the raw imported aliases.
- Unknown formats still fail import with `无效的 api_format: <raw>`.
- A successful import should allow at least one active custom provider/key/model
  path from the imported data to run `/api/admin/provider-query/test-model`;
  failures from OAuth-only auth channels, inactive keys, or unavailable upstream
  services must be reported as model-test runtime failures, not import failures.

### 4. Validation & Error Matrix
- Known Python alias -> normalized canonical signature.
- Known canonical signature -> unchanged canonical signature.
- Known alias on endpoint plus key format -> normalized consistently, no
  missing-format warning.
- Unknown signature -> HTTP `400`, `无效的 api_format: <raw>`.
- Active imported custom provider with working upstream -> test-model HTTP
  `200`, `success=true`.

### 5. Good/Base/Bad Cases
- Good: a Python export with `claude:cli` endpoints imports as
  `claude:messages`, and the matching key formats also normalize.
- Base: Rust-native exports that already use canonical signatures keep working.
- Bad: normalize only endpoint rows while leaving key formats in raw alias form,
  causing model tests to report missing active endpoint/key after import.

### 6. Tests Required
- Unit test the alias mapping table.
- Unit test key-format normalization using raw Python aliases and normalized
  endpoint signatures.
- Live or integration smoke should use a real Python export when changing this
  path; synthetic Rust-shaped fixtures are not enough for migration coverage.

### 7. Wrong vs Correct
#### Wrong
- Validate imported endpoint rows with the raw Python `api_format`, then rely on
  database migrations or later model-test code to clean it up.
#### Correct
- Normalize at import boundary first, then use the normalized signature for
  endpoint persistence, key compatibility, and model-test selection.

## Existing Lint Reality

`lib.rs` currently allows many warnings and clippy lints.

```rust
// apps/aether-gateway/src/lib.rs:1
#![allow(
    dead_code,
    unused_assignments,
    unused_imports,
    unused_mut,
    unused_variables,
    clippy::too_many_arguments,
    clippy::type_complexity,
)]
```

Treat this as a compatibility allowance, not permission to add sloppy code.
New modules should still compile cleanly, minimize unused code, and avoid
needlessly large argument lists unless they are matching an existing gateway
port or handler pattern.

## Acceptable `expect`

The crate uses `expect` for invariant literals and test locks. Keep it there.

```rust
// apps/aether-gateway/src/handlers/proxy/finalize.rs:56
response.headers_mut().insert(
    HeaderName::from_static(TRACE_ID_HEADER),
    HeaderValue::from_str(trace_id).expect("trace id should be a valid header value"),
);
```

Runtime IO, database, network, user input, JSON parsing, and header values from
external requests should return `Result` or direct error responses instead of
panicking.

## Forbidden Patterns

Do not bypass the `control` module with path string checks scattered across
handlers. Route behavior belongs in `control/route/*` plus tests.

Do not perform direct database queries in handlers. Use `AppState` and
`GatewayDataState`.

Do not skip finalization for local responses. You will lose `x-aether-*`
metadata, audit emission, access logs, and request permit handling.

Do not log secrets or raw bodies. Structured ids are enough for request
correlation.

Do not introduce new dependencies without an explicit task. This crate already
depends on many workspace crates; prefer extending local ports and existing
utilities.

Do not add long-lived compatibility passthroughs. Removed passthrough routes
return a `501` with `LOCAL_PROXY_PASSTHROUGH_REMOVED_DETAIL`.

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:1438
let response = build_local_http_error_response(
    &trace_id,
    control_decision,
    http::StatusCode::NOT_IMPLEMENTED,
    LOCAL_PROXY_PASSTHROUGH_REMOVED_DETAIL,
)?;
```

## Review Checklist

- Does the route go through `classify_control_route` and have route tests?
- Does every frontdoor response go through `finalize_gateway_response*`?
- Are errors either `GatewayError` or precise `Response<Body>` values?
- Are persistence calls behind `AppState`/`GatewayDataState`?
- Are structured logs present for operator-relevant failures?
- Are secrets and raw bodies absent from logs and error strings?
- Are streaming responses preserving `text/event-stream`, `Cache-Control`, and
  `x-accel-buffering` behavior?
- Are new async loops bounded by existing timeout, interval, concurrency, or
  backpressure patterns?
