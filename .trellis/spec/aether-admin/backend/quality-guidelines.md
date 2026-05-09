# Quality Guidelines

> Code quality rules for the `aether-admin` Rust crate.

## Scope

This crate is a shared admin helper crate, not an application server. Quality is
measured by stable helper APIs, deterministic transformations, explicit
validation, and small runtime assumptions.

The manifest makes the boundary explicit:

```toml
description = "Shared admin contracts and pure helpers for Aether"
```

Source: `crates/aether-admin/Cargo.toml:7`.

When code needs database connections, application state, authentication
middleware, background jobs, or route registration, place it in a runtime crate
that calls `aether-admin`; do not move those concerns here.

## Required Pattern: Pure Inputs And Outputs

Most public functions accept stored data, JSON values, strings, or request bytes
and return structured values. Keep new helpers in that style.

Good example:

```rust
pub fn build_admin_system_stats_payload(
    total_users: u64,
    active_users: u64,
    total_providers: u64,
    active_providers: u64,
    total_api_keys: u64,
    total_requests: u64,
) -> serde_json::Value {
    json!({
        "users": {
            "total": total_users,
            "active": active_users,
        },
        "providers": {
            "total": total_providers,
            "active": active_providers,
        },
        "api_keys": total_api_keys,
        "requests": total_requests,
    })
}
```

Source: `crates/aether-admin/src/system.rs:633`.

This function has no hidden reads and no side effects. Follow that model for
payload builders.

## Required Pattern: Explicit Field Validation

Reject invalid shapes instead of accepting loose JSON. The crate uses dedicated
parsers and typed request structs.

Examples:

```rust
pub struct AdminSystemSettingsUpdate {
    pub default_provider: Option<Option<String>>,
    pub default_model: Option<Option<String>>,
    pub enable_usage_tracking: Option<bool>,
    pub password_policy_level: Option<String>,
}
```

Source: `crates/aether-admin/src/system.rs:24`.

```rust
pub fn normalize_optional_price(
    value: Option<f64>,
    field_name: &str,
) -> Result<Option<f64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{field_name} 必须是非负数"));
    }
    Ok(Some(value))
}
```

Source: `crates/aether-admin/src/provider/models_write.rs:14`. The validation
rule is exact: finite and non-negative.

Use typed structs when a payload carries multiple fields, and use parse helpers
when the input is query-string or path-like text.

## Required Pattern: Stable Domain Prefixes

Public functions must carry the admin subdomain in the name. This protects
readability in a crate with many public helpers.

Good:

```rust
pub fn admin_monitoring_bad_request_response(detail: impl Into<String>) -> Response<Body>
pub fn admin_pool_sanitize_quick_selectors(selectors: Vec<String>) -> Vec<String>
pub fn admin_provider_ops_sensitive_placeholder_or_empty(
    value: Option<&serde_json::Value>,
) -> bool
```

Sources:

- `crates/aether-admin/src/observability/monitoring.rs:49`
- `crates/aether-admin/src/provider/pool.rs:630`
- `crates/aether-admin/src/provider/ops/config.rs:25`

DON'T add generic public names like `sanitize`, `build_response`, or
`parse_payload`. If a helper is only local, keep it private and name it for the
local algorithm.

## Required Pattern: Deterministic Ordering

Admin payloads should be stable across runs where the input is stable. Use
`BTreeMap` or `BTreeSet` where deterministic output matters.

Example:

```rust
let key_ids = payload
    .key_ids
    .into_iter()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect::<Vec<_>>();
```

Source: `crates/aether-admin/src/provider/pool.rs:562`.

The same pattern appears in query parsing, where `admin_usage_parse_ids` returns
`Option<BTreeSet<String>>` at
`crates/aether-admin/src/observability/usage.rs:82`.

DON'T use `HashSet` or `HashMap` for response ordering unless the output order
is irrelevant and never serialized into admin UI payloads.

## Required Pattern: Privacy And Redaction

This crate shapes admin data, so it must not leak secrets into responses.

Examples:

```rust
const SENSITIVE_SYSTEM_CONFIG_KEYS: &[&str] = &["smtp_password"];

pub fn is_sensitive_admin_system_config_key(key: &str) -> bool {
    SENSITIVE_SYSTEM_CONFIG_KEYS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
}
```

Source: `crates/aether-admin/src/system.rs:521` and
`crates/aether-admin/src/system.rs:1231`.

```rust
fn mask_admin_proxy_node_password(password: Option<&str>) -> Option<String> {
    let password = password?;
    if password.is_empty() {
        return None;
    }
    if password.len() < 8 {
        return Some("****".to_string());
    }
    Some(format!(
        "{}****{}",
        &password[..2],
        &password[password.len() - 2..]
    ))
}
```

Source: `crates/aether-admin/src/system.rs:2163`.

```rust
fn admin_usage_strip_body_ref_metadata(metadata: &mut serde_json::Map<String, Value>) {
    metadata.remove(UsageBodyField::RequestBody.as_ref_key());
    metadata.remove(UsageBodyField::ProviderRequestBody.as_ref_key());
    metadata.remove(UsageBodyField::ResponseBody.as_ref_key());
    metadata.remove(UsageBodyField::ClientResponseBody.as_ref_key());
}
```

Source: `crates/aether-admin/src/observability/usage.rs:282`.

DON'T include raw credentials, proxy passwords, captured body references, or
trace-only metadata in UI payloads without an explicit redaction step.

## Visibility Rules

Use `pub` only for helpers that are consumed outside the module or intentionally
exposed through a facade. Keep algorithmic helpers private.

Good private helpers:

```rust
fn query_param_value(query: Option<&str>, key: &str) -> Option<String>
fn unix_secs_to_rfc3339(unix_secs: u64) -> Option<String>
fn admin_usage_strip_trace_metadata(metadata: &mut serde_json::Map<String, Value>)
```

Sources:

- `crates/aether-admin/src/observability/usage.rs:39`
- `crates/aether-admin/src/observability/usage.rs:52`
- `crates/aether-admin/src/observability/usage.rs:319`

Good restricted helper:

```rust
pub(super) fn json_object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}
```

Source: `crates/aether-admin/src/provider/ops/architectures/mod.rs:190`.

DON'T expose a helper publicly because tests need it. Prefer testing the public
behavior from the module's internal `#[cfg(test)] mod tests`.

## Acceptable Clippy Exceptions

This crate has data-shaping functions that legitimately take many fields. The
existing pattern is to place a narrow `#[allow(clippy::too_many_arguments)]` on
the function, not to disable linting broadly.

Example:

```rust
#[allow(clippy::too_many_arguments)]
pub fn build_admin_provider_endpoint_record(
    id: String,
    provider_id: String,
    normalized_api_format: String,
    api_family: String,
    endpoint_kind: String,
    base_url: String,
    ...
) -> Result<StoredProviderCatalogEndpoint, String>
```

Source: `crates/aether-admin/src/provider/endpoints.rs:136`.

Use this sparingly. If the function starts mixing parsing, business decisions,
and payload construction, split those concerns before adding another allow.

## Testing Requirements

Place focused unit tests at the bottom of the module that owns the behavior.
Current modules use inline `#[cfg(test)] mod tests` blocks:

- `crates/aether-admin/src/system.rs:2178`
- `crates/aether-admin/src/observability/usage.rs:2274`
- `crates/aether-admin/src/observability/stats.rs:2047`
- `crates/aether-admin/src/provider/ops/verify.rs:813`
- `crates/aether-admin/src/provider/status.rs:535`

Test both accepted and rejected input. Good tests assert the semantic detail,
not just that an error exists:

```rust
let err = parse_admin_system_config_import_request(...).expect_err(...);
assert_eq!(err.0, http::StatusCode::BAD_REQUEST);
let detail = err.1["detail"].as_str().expect("detail should be a string");
assert!(detail.contains("providers[0].endpoints[0].is_active"));
```

Source: `crates/aether-admin/src/system.rs:2250`.

For provider architecture registries, test default visibility and fallback
normalization:

```rust
let visible = list_architectures(false);
assert_eq!(visible.len(), 6);
assert!(visible.iter().all(|item| item.architecture_id != "generic_api"));
```

Source: `crates/aether-admin/src/provider/ops/architectures/mod.rs:201`.

## Forbidden Patterns

DON'T add route assembly here:

```rust
// Not an aether-admin responsibility
Router::new().route("/api/admin/system", get(handler))
```

DON'T add database, Redis, or transaction code here. This crate imports stored
record types but does not own persistence execution.

DON'T add `tracing` macros in helpers. Return structured data and let the caller
log with request context.

DON'T silently accept invalid numeric strings in imports. Existing tests require
numeric string fields to fail at
`crates/aether-admin/src/system.rs:2275`.

DON'T add new dependencies for convenience. Prefer existing workspace crates and
standard Rust/serde utilities. The current manifest is intentionally small and
does not include `sea-orm`, `redis`, `tracing`, `anyhow`, or `thiserror`.

## Review Checklist

Reviewers should check:

- The helper belongs in `system`, `observability`, or `provider`.
- Public functions have domain-specific prefixes.
- Invalid input returns `Result` or a response helper, never panic.
- Query and payload parsing distinguishes absent fields from malformed fields.
- Response payloads redact sensitive fields.
- Output ordering is deterministic when serialized to the admin UI.
- Tests cover both success and rejection cases.
- No route registration, database execution, Redis access, tracing, or global
  runtime state is introduced.
