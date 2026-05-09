# Request Execution Guidelines

This guide covers the gateway-specific flow that is not captured by generic
backend structure docs: route classification, request buffering, local/remote
execution, fallback, response finalization, and streaming headers.

## Execution Flow

GitNexus resource traces for repo `Aether` identify the key frontdoor flows:

- `Proxy_request -> Response_is_sse`
- `Proxy_request -> Is_execution_runtime_candidate`

Source inspection shows the flow starts at the catch-all route:

```rust
// apps/aether-gateway/src/router.rs:37
let mut router = router
    .route("/{*path}", any(proxy_request))
    .layer(axum::middleware::from_fn(middleware::access_log_middleware))
    .with_state(state);
```

`proxy_request` owns the high-level sequence:

1. Acquire local/distributed request permits.
2. Extract or generate `trace_id`.
3. Resolve `GatewayPublicRequestContext`.
4. Serve local internal/admin/public support routes.
5. Apply auth, model, and RPM gates.
6. Try local execution runtime stream/sync paths when the route is an AI public
   execution candidate.
7. Try allowed control-execute fallback.
8. Build local miss/removed-passthrough responses when no execution path exists.
9. Finalize the response with trace, control, execution, audit, and access-log
   metadata.

## Control Decisions

All gateway execution depends on `GatewayControlDecision`.

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
    pub(crate) auth_context: Option<GatewayControlAuthContext>,
```

Do not infer AI execution eligibility directly from URL strings in the executor.
Use `decision.is_execution_runtime_candidate()` plus route class/family/kind.

```rust
// apps/aether-gateway/src/control/route/mod.rs:65
pub(crate) fn is_execution_runtime_candidate(&self) -> bool {
    self.execution_runtime_candidate
}
```

The proxy uses this field before attempting local/control execution:

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:968
let should_try_control_execute = control_decision
    .map(|decision| {
        decision.is_execution_runtime_candidate()
            && decision.route_class.as_deref() == Some("ai_public")
    })
    .unwrap_or(false);
```

## Request Buffering

Only buffer bodies when a later gate needs body bytes. Buffering is triggered by
local execution, local auth/model checks, or local public AI support.

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:974
let should_buffer_for_local_ai_public =
    super::public::ai_public_local_requires_buffered_body(&request_context);
let should_buffer_for_local_auth =
    should_buffer_request_for_local_auth(control_decision, &parts.headers);
let should_buffer_body = should_try_control_execute
    || should_buffer_for_local_auth
    || should_buffer_for_local_ai_public;
```

Do not consume `Body` early unless the route is in one of these paths. Axum
bodies are single-use; accidental reads will break proxying.

## Stream Detection

Use `request_wants_stream` for local execution and affinity-forward bridging.
It handles Gemini streaming paths and JSON `stream: true`.

```rust
// apps/aether-gateway/src/handlers/proxy/finalize.rs:18
pub(super) fn request_wants_stream(
    request_context: &GatewayPublicRequestContext,
    body: &axum::body::Bytes,
) -> bool {
    if request_context
        .request_path
        .contains(":streamGenerateContent")
    {
        return true;
    }
    if !request_context
        .request_content_type
        .as_deref()
        .map(|value| value.to_ascii_lowercase().contains("application/json"))
        .unwrap_or(false)
        || body.is_empty()
    {
        return false;
    }
```

Do not reimplement stream detection in new handler code. If a new public AI
surface has a stream signal, extend this helper and the route/planner tests.

## Stream Execution Path

The stream path resolves plan kind, parses the local body, checks whether the
request matches streaming semantics, applies direct-plan bypass, and delegates
to `aether_ai_serving` through `GatewayStreamExecutionPathPort`.

```rust
// apps/aether-gateway/src/executor/stream_path.rs:29
pub(crate) async fn maybe_execute_via_stream_decision_path(
    state: &AppState,
    parts: &http::request::Parts,
    body_bytes: &Bytes,
    trace_id: &str,
    decision: &GatewayControlDecision,
) -> Result<LocalExecutionRequestOutcome, GatewayError> {
    let Some(plan_kind) = resolve_execution_runtime_stream_plan_kind(parts, decision) else {
        return Ok(LocalExecutionRequestOutcome::NoPath);
    };

    let Some((body_json, body_base64)) = parse_local_request_body(parts, body_bytes) else {
        return Ok(LocalExecutionRequestOutcome::NoPath);
    };
```

Execution steps are mapped explicitly:

```rust
// apps/aether-gateway/src/executor/stream_path.rs:93
async fn execute_stream_step(
    &self,
    step: AiStreamExecutionStep,
) -> Result<AiServingExecutionOutcome<Self::Response, Self::Exhaustion>, Self::Error> {
    let outcome = match step {
        AiStreamExecutionStep::LocalVideoContent => {
            maybe_execute_local_video_task_content_stream(
                self.state,
                self.parts,
                self.trace_id,
                self.decision,
                self.plan_kind,
            )
            .await?
        }
```

When adding a new stream execution surface, add a serving step in
`aether-ai-serving` if needed, then map it here to a gateway-local function that
returns `LocalExecutionRequestOutcome`.

## Sync Execution Path

The sync path intentionally rejects matching stream requests before sync
execution, preventing the sync executor from consuming stream traffic.

```rust
// apps/aether-gateway/src/executor/sync_path.rs:53
if let Some(stream_plan_kind) = resolve_execution_runtime_stream_plan_kind(parts, decision) {
    if is_matching_stream_request(stream_plan_kind, parts, &body_json, body_base64.as_deref()) {
        return Ok(LocalExecutionRequestOutcome::NoPath);
    }
}
```

Like the stream path, sync execution uses an adapter port into
`aether_ai_serving`.

```rust
// apps/aether-gateway/src/executor/sync_path.rs:96
impl AiSyncExecutionPathPort for GatewaySyncExecutionPathPort<'_> {
    type Response = Response<Body>;
    type Exhaustion = super::LocalExecutionExhaustion;
    type Error = GatewayError;

    fn scheduler_decision_supported(&self) -> bool {
        self.scheduler_supported
    }
```

## Candidate Loops

Candidate execution loops are shared through `aether_ai_serving::run_ai_attempt_loop`.
Gateway code supplies ports that execute one candidate, mark unused candidates,
and build exhaustion context.

```rust
// apps/aether-gateway/src/executor/candidate_loop.rs:128
impl<T> AiAttemptLoopPort<T> for SyncAttemptLoopPort<'_>
where
    T: AiExecutionAttempt + Send + Sync + 'static,
{
    type Response = Response<Body>;
    type Exhaustion = crate::executor::LocalExecutionExhaustion;
    type Error = GatewayError;

    async fn execute_attempt(&self, attempt: &T) -> Result<Option<Self::Response>, Self::Error> {
        execute_execution_runtime_sync(
            self.state,
            self.parts.uri.path(),
            attempt.execution_plan().clone(),
```

The port contract is the right extension point for per-candidate side effects.
Do not add candidate iteration loops directly in `proxy_request`.

## Planner Boundary

`ai_serving/planner` adapts gateway state and lower-level scheduler/provider
transport logic. It exports plan and attempt builders through `mod.rs`.

```rust
// apps/aether-gateway/src/ai_serving/planner/mod.rs:4
mod candidate_affinity_cache;
mod candidate_materialization;
mod candidate_metadata;
mod candidate_preparation;
mod candidate_ranking;
mod candidate_resolution;
mod candidate_source;
mod candidate_transport_ranking_facts;
mod decision;
mod plan_builders;
mod pool_scheduler;
mod report_context;
mod runtime_miss;
```

Plan-builder functions are re-exported from the planner module rather than
called through deep paths.

```rust
// apps/aether-gateway/src/ai_serving/planner/mod.rs:32
pub(crate) use self::plan_builders::{
    build_gemini_stream_plan_from_decision, build_gemini_sync_plan_from_decision,
    build_openai_responses_stream_plan_from_decision,
    build_openai_responses_sync_plan_from_decision, build_passthrough_sync_plan_from_decision,
    build_standard_stream_plan_from_decision, build_standard_sync_plan_from_decision,
    AiStreamAttempt, AiSyncAttempt,
};
```

When a new model family/provider path needs planning, add it under the planner
family (`standard`, `specialized`, or `passthrough`) and expose it through
`mod.rs`; do not reach into private submodules from executor code.

## Fallback And Miss Handling

Local execution can return `Responded`, `Exhausted`, or `NoPath`.

```rust
// apps/aether-gateway/src/executor/outcome.rs:28
pub(crate) enum LocalExecutionRequestOutcome {
    Responded(Response<Body>),
    Exhausted(LocalExecutionExhaustion),
    NoPath,
}
```

When all local paths miss, the proxy records diagnostics, usage failure, and
fallback metrics before returning a local error response.

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:1284
let local_execution_runtime_miss_diagnostic =
    state.take_local_execution_runtime_miss_diagnostic(&trace_id);
let local_execution_runtime_miss_context =
    build_local_execution_runtime_miss_context(&state, &trace_id, control_decision).await;
let auth_api_key_concurrency_limited = diagnostic_is_auth_api_key_concurrency_limited(
    local_execution_runtime_miss_diagnostic.as_ref(),
) || local_execution_runtime_miss_context
    .all_candidates_skipped_for_reason("api_key_concurrency_limit_reached");
```

Do not collapse exhausted and no-path cases. Exhausted requests can include
last failed candidate information and should be recorded differently from a pure
runtime miss.

## Response Building And SSE

All upstream responses should flow through `api::response` helpers so skipped
headers, streaming headers, trace headers, gateway marker, and control metadata
are consistent.

```rust
// apps/aether-gateway/src/api/response.rs:97
pub(crate) fn build_client_response_from_parts_with_mutator<F>(
    status_code: u16,
    upstream_headers: &BTreeMap<String, String>,
    body: Body,
    trace_id: &str,
    control_decision: Option<&GatewayControlDecision>,
    mutate_headers: F,
) -> Result<Response<Body>, GatewayError>
where
    F: FnOnce(&mut http::HeaderMap) -> Result<(), GatewayError>,
```

SSE responses must disable buffering and transformation:

```rust
// apps/aether-gateway/src/api/response.rs:39
pub(crate) fn apply_streaming_response_headers(headers: &mut http::HeaderMap) {
    if !response_is_sse(headers) {
        return;
    }

    headers.insert(
        http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
}
```

Do not return `text/event-stream` bodies without passing through this path.

## Finalization

Every response path in `proxy_request` should return through
`finalize_gateway_response_with_context` or `finalize_gateway_response`.

```rust
// apps/aether-gateway/src/handlers/proxy/finalize.rs:235
pub(super) fn finalize_gateway_response_with_context(
    state: &AppState,
    response: Response<Body>,
    remote_addr: &std::net::SocketAddr,
    request_context: &GatewayPublicRequestContext,
    execution_path: &'static str,
    started_at: &Instant,
    request_permit: Option<AdmissionPermit>,
) -> Response<Body> {
    finalize_gateway_response(
        state,
        response,
        &request_context.trace_id,
```

Finalization attaches route headers only when absent, allowing specialized
responses to override them safely.

```rust
// apps/aether-gateway/src/handlers/proxy/finalize.rs:171
fn attach_control_decision_headers(
    response: &mut Response<Body>,
    control_decision: Option<&GatewayControlDecision>,
) {
    let Some(control_decision) = control_decision else {
        return;
    };
    if !response.headers().contains_key(CONTROL_ROUTE_CLASS_HEADER) {
        response.headers_mut().insert(
```

## DON'T

Do not add direct upstream proxy fallback for removed compatibility paths. The
current behavior returns a local `501`:

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:1438
let response = build_local_http_error_response(
    &trace_id,
    control_decision,
    http::StatusCode::NOT_IMPLEMENTED,
    LOCAL_PROXY_PASSTHROUGH_REMOVED_DETAIL,
)?;
```

Do not buffer bodies, execute candidates, or mutate response headers in helper
code that cannot see `trace_id`, `GatewayControlDecision`, and execution path.

Do not add streaming conversions outside the established bridge/finalize helpers
such as `maybe_bridge_standard_sync_json_to_stream`,
`aggregate_*_stream_sync_response`, and `apply_streaming_response_headers`.
