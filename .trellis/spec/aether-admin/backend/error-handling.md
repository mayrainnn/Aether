# Error Handling

> Error-return conventions for `crates/aether-admin`.

## Scope

`aether-admin` does not define a single crate-wide error enum. It uses small,
domain-specific return types because the crate mostly parses request-like input,
builds response payloads, and constructs stored records for other layers.

There are two dominant error shapes:

- `(http::StatusCode, serde_json::Value)` for helpers that already know the
  admin API response shape.
- `String` for lower-level validation, normalization, and provider operation
  helpers that callers wrap into HTTP responses.

Do not add `anyhow::Result` or a generic boxed error to this crate. The public
surface is deliberately simple and serializable.

## API-Response Errors

Use `(http::StatusCode, serde_json::Value)` when a helper validates admin API
request bytes and can return the exact client-facing JSON error body.

The local `invalid_request` helper centralizes the common bad-request shape:

```rust
fn invalid_request(detail: impl Into<String>) -> (http::StatusCode, serde_json::Value) {
    (
        http::StatusCode::BAD_REQUEST,
        json!({ "detail": detail.into() }),
    )
}
```

Source: `crates/aether-admin/src/system.rs:64`.

Representative parser:

```rust
pub fn parse_admin_system_settings_update(
    request_body: &[u8],
) -> Result<AdminSystemSettingsUpdate, (http::StatusCode, serde_json::Value)> {
    let payload = match serde_json::from_slice::<serde_json::Value>(request_body) {
        Ok(serde_json::Value::Object(payload)) => payload,
        Ok(_) | Err(_) => {
            return Err((
                http::StatusCode::BAD_REQUEST,
                json!({ "detail": "请求数据验证失败" }),
            ));
        }
    };
    ...
}
```

Source: `crates/aether-admin/src/system.rs:669`. The source message is localized
for the product UI; the important convention is the `StatusCode` plus
`{"detail": ...}` shape.

Use this pattern for system settings, config import/export, email-template
payloads, and other helpers that operate directly on raw request bytes.

## Response Helper Errors

When the result is already an HTTP response, return `Response<Body>` and build
it through Axum's `(StatusCode, Json(...)).into_response()` tuple conversion.

Example:

```rust
pub fn admin_usage_bad_request_response(detail: impl Into<String>) -> Response<Body> {
    (
        http::StatusCode::BAD_REQUEST,
        Json(json!({ "detail": detail.into() })),
    )
        .into_response()
}
```

Source: `crates/aether-admin/src/observability/usage.rs:31`.

Monitoring follows the same rule:

```rust
pub fn admin_monitoring_not_found_response(detail: &'static str) -> Response<Body> {
    (
        http::StatusCode::NOT_FOUND,
        Json(json!({ "detail": detail })),
    )
        .into_response()
}
```

Source: `crates/aether-admin/src/observability/monitoring.rs:57`.

Keep response builders deterministic: no logging, no database lookup, and no
state access inside the error helper.

## String Validation Errors

Use `Result<T, String>` for reusable domain validation that is not inherently
HTTP-specific.

Provider model write helpers use plain strings:

```rust
pub fn normalize_required_trimmed_string(value: &str, field_name: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field_name} 不能为空"));
    }
    Ok(trimmed.to_string())
}
```

Source: `crates/aether-admin/src/provider/models_write.rs:6`. Callers should
preserve the message or map it to a standard admin response.

Provider pool planning also returns `String` because it produces a plan, not an
HTTP response:

```rust
pub fn build_admin_pool_batch_action_plan(
    payload: AdminPoolBatchActionRequest,
) -> Result<AdminPoolBatchActionPlan, String> {
    ...
    if key_ids.is_empty() {
        return Err("key_ids should not be empty".to_string());
    }
    ...
}
```

Source: `crates/aether-admin/src/provider/pool.rs:541`.

Use `String` when the helper could be reused by CLI tests, handlers, background
jobs, or future admin runtimes without importing HTTP semantics.

## Boundary Parsing

Validation helpers should parse at the boundary and reject ambiguous input early.

Examples:

```rust
pub fn parse_bounded_u32(field: &str, value: &str, min: u32, max: u32) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("{field} must be a valid integer"))?;
    if parsed < min || parsed > max {
        return Err(format!("{field} must be between {min} and {max}"));
    }
    Ok(parsed)
}
```

Source: `crates/aether-admin/src/observability/stats.rs:142`.

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

Do not coerce invalid request values into defaults. Defaults are used when a
field is absent, not when a provided field is malformed.

## Error Conversion From Contract Types

This crate often delegates final validation to stored record constructors from
`aether-data-contracts` and converts those domain errors to strings:

```rust
StoredProviderCatalogEndpoint::new(
    id,
    provider_id,
    normalized_api_format,
    Some(api_family),
    Some(endpoint_kind),
    true,
)
.map_err(|err| err.to_string())?
.with_timestamps(Some(now_unix_secs), Some(now_unix_secs))
.with_transport_fields(...)
.map_err(|err| err.to_string())
```

Source: `crates/aether-admin/src/provider/endpoints.rs:153`.

Keep this conversion narrow. The lower contract crate validates invariants; the
admin crate preserves its reason as a user-facing validation string. Do not
discard the message with a generic `"invalid request"` unless the source error
would reveal sensitive data.

## Detailed Import Errors

For import payloads, preserve field paths when shape validation fails. The tests
lock this behavior:

```rust
let detail = err.1["detail"].as_str().expect("detail should be a string");
assert!(detail.contains("providers[0].endpoints[0].is_active"));
```

Source: `crates/aether-admin/src/system.rs:2269`.

Use `serde_path_to_error` or equivalent path-preserving validation for nested
config imports. Do not replace it with a plain `serde_json::from_value(...)?`
path if that would hide which field failed.

## Error Handling Anti-Patterns

DON'T log and return from this crate:

```rust
// DON'T in aether-admin helpers
tracing::warn!(error = %err, "invalid admin request");
Err(err)
```

The caller owns request id, user id, route, redaction policy, and severity.
Return structured errors and let the runtime layer log.

DON'T use `unwrap()` or `expect()` in public helpers. Tests use `expect` for
fixtures, for example `crates/aether-admin/src/observability/stats.rs:2096`,
but production helpers return `Option` or `Result`.

DON'T add a third ad-hoc error shape such as `Result<T, serde_json::Value>` or
`Result<T, (u16, String)>`. Choose one of the existing two patterns:
`(StatusCode, Value)` for API-shaped failures, `String` for reusable validation.

DON'T silently sanitize away invalid input. `parse_admin_system_config_import`
tests reject numeric strings such as `"1.80000000"` for numeric fields and assert
that the failure mentions the offending field at
`crates/aether-admin/src/system.rs:2275`.

## Checklist

For every new fallible helper:

- Does the caller need an HTTP-ready error body? Use
  `Result<T, (http::StatusCode, serde_json::Value)>`.
- Is the helper reusable outside HTTP? Use `Result<T, String>`.
- Is absent input distinct from malformed input?
- Does nested import validation preserve the failing field path?
- Are sensitive values omitted from the error message?
- Is the error returned instead of logged?
