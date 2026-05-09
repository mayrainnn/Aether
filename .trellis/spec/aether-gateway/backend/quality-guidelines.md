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
