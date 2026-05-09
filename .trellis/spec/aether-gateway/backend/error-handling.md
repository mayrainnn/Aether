# Error Handling

`apps/aether-gateway` uses a small crate-local error type for fallible gateway
paths and direct `Response<Body>` values for client validation responses that
need precise HTTP status/body control.

## Primary Error Type

The main error type is `GatewayError`. It is intentionally crate-private and
implements axum `IntoResponse`.

```rust
// apps/aether-gateway/src/error.rs:12
#[derive(Debug)]
pub(crate) enum GatewayError {
    UpstreamUnavailable { trace_id: String, message: String },
    ControlUnavailable { trace_id: String, message: String },
    Internal(String),
}
```

Public handlers should return `Result<Response<Body>, GatewayError>` when the
error can be represented as one of these variants.

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:670
pub(crate) async fn proxy_request(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<std::net::SocketAddr>,
    request: Request,
) -> Result<Response<Body>, GatewayError> {
```

## Client Surface For GatewayError

`UpstreamUnavailable` and `ControlUnavailable` both surface as `502 Bad Gateway`,
emit a warning with `trace_id`, and attach gateway/trace headers. `Internal`
surfaces as `500`.

```rust
// apps/aether-gateway/src/error.rs:19
impl IntoResponse for GatewayError {
    fn into_response(self) -> Response<Body> {
        match self {
            Self::UpstreamUnavailable { trace_id, message } => {
                warn!(trace_id = %trace_id, error = %message, "gateway proxy unavailable");
                let body = Json(json!({
                    "error": {
                        "message": "gateway proxy unavailable",
                        "trace_id": trace_id,
                    }
                }));
```

The client response deliberately hides the raw upstream/control error message
for these variants, while the log keeps it for operators.

## Conversion Pattern

Only add `From` implementations when the conversion is lossless enough for the
gateway boundary. The current crate converts AI surface finalize failures into
internal gateway failures.

```rust
// apps/aether-gateway/src/error.rs:71
impl From<AiSurfaceFinalizeError> for GatewayError {
    fn from(error: AiSurfaceFinalizeError) -> Self {
        GatewayError::Internal(error.0)
    }
}
```

Most external errors are mapped at the boundary with context:

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:437
let upstream_response = upstream_request
    .body(buffered_body.cloned().unwrap_or_default())
    .send()
    .await
    .map_err(|err| GatewayError::UpstreamUnavailable {
        trace_id: request_context.trace_id.clone(),
        message: format!("owner gateway affinity forward failed: {err}"),
    })?;
```

Use `GatewayError::UpstreamUnavailable` for upstream/network failures that make
the proxy unable to produce a provider response. Use `GatewayError::Internal`
for serialization, invalid header construction, state facade, and invariant
failures.

## Propagation Pattern

The normal path is `?` through async gateway layers, not nested matches.

```rust
// apps/aether-gateway/src/control/route/mod.rs:138
pub(crate) async fn resolve_control_route(
    state: &AppState,
    method: &http::Method,
    uri: &Uri,
    headers: &http::HeaderMap,
    trace_id: &str,
) -> Result<Option<GatewayControlDecision>, GatewayError> {
    let Some(mut decision) = classify_control_route(method, uri, headers) else {
        return Ok(None);
    };
    decision.public_query_string = uri.query().map(ToOwned::to_owned);

    match resolve_control_decision_auth(state, headers, uri, trace_id, decision).await? {
        ControlDecisionAuthResolution::Resolved(decision) => Ok(Some(decision)),
    }
}
```

Data-layer facade calls generally map repository errors into `GatewayError` at
the `AppState` boundary.

```rust
// apps/aether-gateway/src/state/runtime/gemini_files.rs:4
pub(crate) async fn upsert_gemini_file_mapping(
    &self,
    record: aether_data::repository::gemini_file_mappings::UpsertGeminiFileMappingRecord,
) -> Result<
    Option<aether_data::repository::gemini_file_mappings::StoredGeminiFileMapping>,
    GatewayError,
> {
    self.data
        .upsert_gemini_file_mapping(record)
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))
}
```

## Local HTTP Errors

For expected client-visible gateway decisions, build a response directly and
finalize it instead of returning a generic `GatewayError`. This keeps status
codes and execution-path headers precise.

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:1116
if control_decision.is_none() {
    let response = build_local_http_error_response(
        &trace_id,
        None,
        http::StatusCode::NOT_FOUND,
        LOCAL_ROUTE_NOT_FOUND_DETAIL,
    )?;
    return Ok(finalize_gateway_response_with_context(
        &state,
        response,
        &remote_addr,
        &request_context,
        EXECUTION_PATH_LOCAL_ROUTE_NOT_FOUND,
        &started_at,
        request_permit.take(),
    ));
}
```

Other examples of direct responses include local auth rejection, RPM limiting,
overload handling, route-not-found, removed passthrough, and local execution
runtime miss responses.

## Error Logging

Do not log every propagated error at the point it is created. Log at the
operator boundary where the context is complete:

- `GatewayError::IntoResponse` logs unavailable gateway/control failures.
- `finalize_gateway_response` logs request failures once with route and
  execution-path metadata.
- Background workers log failures through `log_maintenance_worker_failure`.

```rust
// apps/aether-gateway/src/maintenance/runtime/workers.rs:31
fn log_maintenance_worker_failure(
    worker: &'static str,
    phase: &'static str,
    error: &impl std::fmt::Debug,
) {
    warn!(
        event_name = "maintenance_worker_failed",
        log_type = "ops",
        worker,
        phase,
        error = ?error,
        "gateway maintenance worker failed"
    );
}
```

## DON'T

Do not return `GatewayError::Internal` for expected authorization, route, model,
or quota denials. Build the specific response and finalize it.

```rust
// DON'T
return Err(GatewayError::Internal("route not found".to_string()));
```

Use the existing response helper and execution-path label.

Do not expose provider credentials, management tokens, OAuth tokens, or raw
request bodies in error strings. Error messages may be logged or returned to
clients depending on the variant.

Do not use `anyhow::Error` in gateway handler signatures. Keep handler errors
convertible to `GatewayError` or explicit `Response<Body>` so axum responses
remain predictable.
