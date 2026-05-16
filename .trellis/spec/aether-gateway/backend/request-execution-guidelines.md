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

## Public Image Guardrails

`/v1/images/generations` and `/v1/images/edits` are public OpenAI Images
surfaces. Validate request shape before candidate execution when the rule is
independent of provider secrets or upstream state. For generation count, use the
requested model as the best available public signal: Grok image models may use
the wider Grok ceiling, while other or missing models must keep the default
OpenAI-compatible ceiling and fail before the request enters candidate
selection.

Do not solve model-specific image limits by letting non-matching candidates
skip later and surface as generic candidate exhaustion. That makes user errors
look like provider availability failures.

## Browser-Impersonation Transport

Some upstreams are browser-sensitive even after Aether has built the correct
provider URL, headers, and body. These plans must express the browser behavior
through `ExecutionPlan.transport_profile`, not through provider-specific code in
the generic sync or stream executor.

### 1. Scope / Trigger

- Trigger: an execution plan needs browser TLS/HTTP fingerprint behavior, for
  example Grok Web app-chat requests.
- The browser transport is a runtime transport capability. Grok may be the first
  provider using it, but the execution runtime must branch on transport backend
  constants rather than provider names.

### 2. Signatures

- `send_request(plan, body_bytes)` dispatches by
  `ResolvedTransportProfile.backend`.
- Supported browser backends:
  - `TRANSPORT_BACKEND_BROWSER_WREQ` -> in-process Rust `wreq` client.
- `DirectHttpResponse` must wrap both `reqwest::Response` and `wreq::Response`
  so sync executors can collect status, headers, and body without duplicating
  provider logic.
- `DirectUpstreamResponse` must include the same backend variants for streaming
  pumps.

### 3. Contracts

- `browser_wreq` sends directly to `plan.url`. It forwards the sanitized plan
  headers and the already-built request body.
- Execution control headers such as `x-aether-execution-follow-redirects` and
  `x-aether-execution-http1-only` must be consumed by the gateway and must not
  leak to the upstream target.
- Redirects are disabled unless
  `x-aether-execution-follow-redirects: true` is present. If enabled, the Rust
  browser backend uses a bounded redirect policy.
- Proxy, connect timeout, total timeout, read timeout, HTTP/1-only, and
  invalid-certificate behavior must be honored consistently by browser and
  non-browser direct transports where the underlying client supports them.
- Browser impersonation profile names must be fail-loud. If a plan asks for an
  unsupported `browser_profile` / `impersonate` value, return
  `UnsupportedTransportProfile`; do not silently downgrade to a different
  browser fingerprint.

### 4. Validation & Error Matrix

- Unknown transport backend -> `UnsupportedTransportProfile`.
- Invalid upstream method -> `InvalidMethod`.
- Invalid proxy URL -> `InvalidProxy` for `reqwest`, `BrowserClientBuild` for
  `wreq`.
- Browser client construction failure -> `BrowserClientBuild`.
- Unknown browser impersonation profile -> `UnsupportedTransportProfile`.
- Browser body collection failure -> `BrowserBody`.
- Upstream request failure -> `UpstreamRequest` with backend-specific error
  formatting.

### 5. Good/Base/Bad Cases

- Good: Grok fixed-provider transport resolves to `browser_wreq`, then the
  Grok runtime converts OpenAI/Claude-compatible client bodies to Grok app-chat
  before `send_request`.
- Base: an account includes legacy `browser_transport_backend` fields; provider
  transport ignores them and still resolves to the Rust `browser_wreq` backend.
- Bad: a provider-specific executor calls `reqwest` directly or leaks
  `x-aether-execution-*` headers upstream.

### 6. Tests Required

- A direct sync execution test that routes through `browser_wreq` in-process and
  asserts the upstream receives the original body and no internal control
  headers.
- A Grok runtime-marker test proving model-test/gateway plans convert through
  the Grok runtime before browser transport sends bytes upstream.
- Provider-transport tests proving Grok defaults to `browser_wreq`.

### 7. Wrong vs Correct

Wrong: make Grok look like a standard OpenAI-compatible local transport just so
the model-test path can execute it.

Correct: keep Grok's provider protocol conversion in the Grok runtime, and keep
browser fingerprinting as a reusable execution transport backend selected by
`ResolvedTransportProfile.backend`.

Grok text candidates are provider-specific even when the catalog endpoint is
advertised as `openai:chat`, `openai:responses`, or `claude:messages`. Planner
code must preserve the original client body shape and attach Grok browser-auth
headers; it must not pre-convert OpenAI Responses or Claude Messages into a
standard provider body before the Grok runtime runs. This mirrors Kiro's
provider-specific model-test pattern rather than Codex's standard
OpenAI-Responses path.

## Grok Attachment Uploads

Grok multimodal input is not an Aether file-storage feature. The gateway may
temporarily parse client `image_url`, `input_image`, `file`, `input_file`,
Claude `image`, and Claude `document` blocks, but it must not persist those
inputs to Aether storage or create durable Aether asset records.

The runtime flow is:

1. Build the normal Grok app-chat body from the client request.
2. Extract attachment inputs from the original client body.
3. Resolve each attachment in memory only:
   - `data:` URI -> validate base64 and MIME type.
   - `http` / `https` URL -> resolve to public IP addresses only, pin the
     checked address into the HTTP client, and repeat the same check for every
     redirect before downloading with a bounded timeout, redirect limit, and byte
     limit.
4. Upload the base64 payload to Grok `/rest/app-chat/upload-file` using the same
   Grok browser-auth headers and transport backend as the app-chat request.
5. Insert returned Grok `fileMetadataId` values into `fileAttachments`.

The prompt conversion must keep only user-visible text content. Do not include
`[image: ...]`, raw data URIs, or downloaded file content in the Grok `message`
when the same attachment is uploaded through `fileAttachments`.

Validation rules:

- Unsupported attachment schemes must fail loudly; do not silently drop them.
- Remote attachment URLs that resolve to loopback, private, link-local,
  documentation, unspecified, or other non-public addresses must fail before any
  bytes are fetched. Redirect targets must pass the same check.
- Attachment downloads must enforce an in-memory byte limit before full
  collection.
- Upload responses without `fileMetadataId` / `fileId` are errors.
- Aether must not write input attachments to local disk, object storage, DB
  tables, or reusable cache as part of this runtime path.

## Grok Image Generation And Edit

Grok image requests use Aether's `openai:image` surface, but their capability
limits are provider-specific:

- Aether's public image surface is `POST /v1/images/generations` and
  `POST /v1/images/edits`. Do not mount or classify
  `POST /v1/images/variations` as a public AI route, even if an upstream has a
  similarly named legacy capability.
- Public route validation may enforce a shared upper bound such as `n=1..4` to
  reject obviously invalid requests early, but provider-specific generation
  count behavior belongs to the image normalizer/runtime, not to the public
  route guard.
- The generic OpenAI image normalizer remains conservative and only accepts
  `n=1` by default. Do not globally loosen this rule for every image provider.
- Grok generation candidates may opt into the shared image capability helper's
  larger generation count ceiling. The normalized provider body must preserve
  `n` so the Grok runtime can request multiple images from Imagine WebSocket.
- Grok image edit still requires reference images and should stay single-request
  from the client perspective; the Grok edit body itself may set the upstream
  `imageGenerationCount` needed by the Web protocol.
- Size/aspect controls are carried through the OpenAI image provider body under
  `tools[0]`. Grok runtime helpers must read `tools[0].size`,
  `tools[0].aspect_ratio`, `tools[0].ratio`, and top-level fallbacks before
  defaulting.

## Grok Client Response Surfaces

Grok upstream always returns Grok Web app-chat events, but Aether's public
response must match the original client endpoint format for both sync and
streaming requests. For text surfaces, Grok should first normalize its result
into an OpenAI Responses sync body and then reuse the Aether canonical
conversion/stream bridge to emit the requested client format.

- `openai:chat` stream responses must use OpenAI Chat Completions chunk shape.
- `openai:responses` stream responses must use Responses `response.*` events.
- `claude:messages` stream responses must use Claude Messages SSE events such
  as `message_start`, `content_block_delta`, `message_delta`, and
  `message_stop`; every data payload must include the Claude `type`
  discriminator expected by Claude clients.
- `openai:image` stream responses must use image generation events.

Do not let a new client format fall through to the OpenAI Chat default branch.
Usage/runtime logging can still succeed when the client receives the wrong SSE
shape, so regression tests must assert the external event shape, not only the
upstream execution result.

## Grok Provider-Pool Quota

Grok quota is a provider-pool capability, not a UI-only display rule. Keep the
tier boundary in the provider-pool helper layer and let gateway refresh/report
effects consume that boundary.

- `basic` accounts should only expose and evaluate the `fast` quota window.
- `super` accounts should expose `auto`, `fast`, `expert`, and `grok_4_3`.
- `heavy` accounts should expose `auto`, `fast`, `expert`, `heavy`, and
  `grok_4_3`.
- Quota refresh may use existing key status/upstream metadata/auth config to
  decide which Grok `/rest/rate-limits` modes to query. If the tier is not yet
  known, a bootstrap refresh may query the broad mode set and persist the
  inferred tier for later narrow refreshes.
- Runtime report effects should apply local feedback like other provider-pool
  quota paths: successful Grok requests decrement the matching mode window,
  while 429 responses set that mode window to exhausted. If the upstream Grok
  error text includes a wait duration such as "等待 6小时 13分钟" or "wait 6h
  13m", treat that duration as the more precise reset time for the matching
  window and persist it as `reset_at` / `next_reset_at` / `reset_after_seconds`.
  The next explicit refresh can overwrite the estimate with Grok's authoritative
  live value.
- Quota snapshots should expose reset information at both the aggregate quota
  level and, when available, individual model windows. UI consumers must fall
  back from a window's missing `reset_at` / `reset_seconds` to the aggregate
  quota-level values instead of hiding Grok reset countdowns.
- Provider-pool payloads must keep quota refresh separate from OAuth token
  refresh. A Grok cookie/sso import is still an OAuth-managed session, but
  `can_refresh_oauth` must be `false` unless decrypted `auth_config` has a
  non-empty `refresh_token`, matching `/provider-oauth/keys/:id/refresh`.

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
