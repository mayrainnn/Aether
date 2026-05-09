# Logging Guidelines

> Logging and observability boundaries for `crates/aether-admin`.

## Scope

`aether-admin` currently does not log. This is intentional for the crate's
current role as a shared admin helper and contract crate.

The manifest has no `tracing` dependency:

```toml
[dependencies]
aether-ai-formats.workspace = true
aether-billing.workspace = true
aether-contracts.workspace = true
aether-data.workspace = true
aether-data-contracts.workspace = true
axum.workspace = true
base64.workspace = true
chrono.workspace = true
http.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
serde_path_to_error.workspace = true
sha2.workspace = true
url.workspace = true
uuid.workspace = true
```

Source: `crates/aether-admin/Cargo.toml:9`.

Repository scan evidence: no `tracing`, `debug!`, `info!`, `warn!`, or
`error!` calls exist under `crates/aether-admin/src/`.

## Rule: Return Facts, Let Callers Log

Helpers in this crate usually return one of these:

- `serde_json::Value`
- `Response<Body>`
- `Result<T, String>`
- `Result<T, (http::StatusCode, serde_json::Value)>`
- typed structs/enums such as `AdminMonitoringRoute` or
  `AdminPoolBatchActionPlan`

Those return values are what runtime code should log after it attaches request
context, user context, route, trace id, and redaction policy.

Example fact-returning parser:

```rust
pub fn admin_usage_parse_limit(query: Option<&str>) -> Result<usize, String> {
    match query_param_value(query, "limit") {
        None => Ok(100),
        Some(value) => {
            let parsed = value
                .parse::<usize>()
                .map_err(|_| "limit must be a positive integer".to_string())?;
            if parsed == 0 || parsed > 500 {
                return Err("limit must be between 1 and 500".to_string());
            }
            Ok(parsed)
        }
    }
}
```

Source: `crates/aether-admin/src/observability/usage.rs:58`.

Do not insert logging into this function. The caller can log the route, query
string policy, admin identity, and returned error without making this shared
crate depend on runtime context.

## Rule: Response Helpers Are Not Log Sites

Response helper functions should only shape the response:

```rust
pub fn admin_monitoring_bad_request_response(detail: impl Into<String>) -> Response<Body> {
    (
        http::StatusCode::BAD_REQUEST,
        Json(json!({ "detail": detail.into() })),
    )
        .into_response()
}
```

Source: `crates/aether-admin/src/observability/monitoring.rs:49`.

```rust
pub fn admin_usage_data_unavailable_response(detail: &'static str) -> Response<Body> {
    (
        http::StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "detail": detail })),
    )
        .into_response()
}
```

Source: `crates/aether-admin/src/observability/usage.rs:23`.

DON'T add logs inside these helpers:

```rust
// DON'T in aether-admin
pub fn admin_monitoring_bad_request_response(detail: impl Into<String>) -> Response<Body> {
    let detail = detail.into();
    tracing::warn!(%detail, "bad admin monitoring request");
    (http::StatusCode::BAD_REQUEST, Json(json!({ "detail": detail }))).into_response()
}
```

A response helper cannot know whether a bad request is suspicious, expected UI
validation, an integration test, or a repeated production failure.

## Logging Locations Outside This Crate

If a caller needs logs, log around the call site where context exists.

Suggested caller-side fields:

- route or enum variant returned by `match_admin_monitoring_route`;
- request id and trace id from the runtime layer;
- authenticated admin/user id from middleware;
- provider id, endpoint id, key id, or model id after redaction;
- validation error string or HTTP status code;
- count/limit/offset values after parsing.

For example, caller-side logging around the route classifier could use:

```rust
let route = match_admin_monitoring_route(method, path);
debug!(
    method = %method,
    path = %path,
    route = ?route,
    "classified admin monitoring route"
);
```

This example belongs in the runtime/admin router crate, not in
`aether-admin`. The classifier itself is pure:

```rust
pub fn match_admin_monitoring_route(
    method: &http::Method,
    path: &str,
) -> Option<AdminMonitoringRoute> {
    let path = normalize_admin_monitoring_path(path);
    ...
}
```

Source: `crates/aether-admin/src/observability/monitoring.rs:997`.

## Sensitive Data Rules

Do not log raw admin payloads, provider credentials, proxy URLs with credentials,
captured request/response bodies, or internal routing metadata from this crate.

The crate already strips or masks several sensitive fields before building UI
payloads:

```rust
fn admin_usage_strip_body_ref_metadata(metadata: &mut serde_json::Map<String, Value>) {
    metadata.remove(UsageBodyField::RequestBody.as_ref_key());
    metadata.remove(UsageBodyField::ProviderRequestBody.as_ref_key());
    metadata.remove(UsageBodyField::ResponseBody.as_ref_key());
    metadata.remove(UsageBodyField::ClientResponseBody.as_ref_key());
}
```

Source: `crates/aether-admin/src/observability/usage.rs:282`.

```rust
fn admin_usage_strip_routing_metadata(metadata: &mut serde_json::Map<String, Value>) {
    metadata.remove("candidate_id");
    metadata.remove("candidate_index");
    metadata.remove("key_name");
    metadata.remove("model_id");
    metadata.remove("global_model_id");
    metadata.remove("global_model_name");
    metadata.remove("planner_kind");
    metadata.remove("route_family");
    metadata.remove("route_kind");
    metadata.remove("execution_path");
    metadata.remove("local_execution_runtime_miss_reason");
}
```

Source: `crates/aether-admin/src/observability/usage.rs:289`.

```rust
fn admin_usage_strip_trace_metadata(metadata: &mut serde_json::Map<String, Value>) {
    metadata.remove("trace_id");
}
```

Source: `crates/aether-admin/src/observability/usage.rs:319`.

```rust
pub fn admin_provider_ops_sensitive_placeholder_or_empty(
    value: Option<&serde_json::Value>,
) -> bool {
    match value {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::String(raw)) => raw.is_empty() || raw.chars().all(|ch| ch == '*'),
        Some(serde_json::Value::Array(items)) => items.is_empty(),
        Some(serde_json::Value::Object(map)) => map.is_empty(),
        _ => false,
    }
}
```

Source: `crates/aether-admin/src/provider/ops/config.rs:25`.

Caller logs should use the sanitized payloads or identifiers, not the raw input
objects that these helpers receive.

## What To Log In Callers

Because this crate is log-free, the following is caller-side guidance:

- `debug`: route classification, normalized filters, selected provider-ops
  architecture, sanitized quick selectors, and count/limit pagination decisions.
- `info`: successful admin operations that mutate data, such as completed batch
  actions or config import/export jobs.
- `warn`: rejected admin inputs, provider verification failures, invalid
  connector credentials, unavailable admin data, or route drift between the real
  router and helper classifiers.
- `error`: persistence failures, transaction failures, unavailable required
  services, or corruption detected by callers before/after using these helpers.

Do not add this policy as code in `aether-admin`; document it for callers and
keep the helper crate dependency-light.

## Anti-Patterns

DON'T add a `tracing` dependency just to debug a parser:

```toml
# DON'T in crates/aether-admin/Cargo.toml
tracing.workspace = true
```

DON'T log raw provider operation credentials. `provider/ops/verify.rs` handles
cookies and auth-derived headers, for example `build_headers` at
`crates/aether-admin/src/provider/ops/verify.rs:15`. Logs around those calls
must redact credential fields.

DON'T log request bodies or response bodies from usage records. The usage module
explicitly strips body-reference metadata before presenting records to the admin
UI at `crates/aether-admin/src/observability/usage.rs:282`.

DON'T log inside tests as a substitute for assertions. Existing tests assert
exact behavior, such as import failures containing field paths at
`crates/aether-admin/src/system.rs:2250`.

## If Logging Is Ever Added

Only add logging to this crate if its role changes from pure helper code to
runtime execution code. That change should come with all of the following:

- a clear reason in the design/task document;
- `tracing` added deliberately to `Cargo.toml`;
- structured fields only, no raw JSON payload dumps;
- tests or review notes proving sensitive values are redacted;
- caller-side guidance updated to avoid duplicate logs;
- a narrow log level policy for each new log site.

Until then, the correct pattern is no logs in `aether-admin`.
