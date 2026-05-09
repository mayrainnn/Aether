# Aether Admin Backend Guidelines

> Project-specific coding guidance for `crates/aether-admin`.

## Crate Summary

`aether-admin` is the shared admin contracts and pure-helper crate for Aether.
It is in application layer terms because it shapes admin-facing behavior, but it
does not assemble the admin HTTP router and does not execute persistence.

Evidence:

```toml
description = "Shared admin contracts and pure helpers for Aether"
```

Source: `crates/aether-admin/Cargo.toml:7`.

The crate root exports exactly three domains:

```rust
pub mod observability;
pub mod provider;
pub mod system;
```

Source: `crates/aether-admin/src/lib.rs:1`.

## Final Guide Set

| Guide | Status | Purpose |
| --- | --- | --- |
| [Directory Structure](./directory-structure.md) | Complete | Explains the real `system`, `observability`, and `provider` domains, provider-ops facade, and where new helpers belong. |
| [Error Handling](./error-handling.md) | Complete | Documents the two current error shapes: `(StatusCode, Value)` for API-shaped failures and `String` for reusable validation. |
| [Quality Guidelines](./quality-guidelines.md) | Complete | Captures pure-helper boundaries, deterministic output, naming, visibility, redaction, testing, and forbidden runtime concerns. |
| [Logging Guidelines](./logging-guidelines.md) | Complete | Documents the current no-logging boundary and caller-side logging policy. |

`database-guidelines.md` was intentionally removed. This crate does not use
SeaORM, Redis, transactions, migrations, or connection pools. It imports stored
record types from `aether-data` and `aether-data-contracts`, but persistence is a
caller/runtime responsibility.

## What This Crate Should Contain

Add code here when it is one of these:

- admin request parsing or validation;
- admin UI JSON response shaping;
- Axum-compatible response helper that does not access app state;
- provider model, endpoint, key-pool, quota, account-status, OAuth, or provider
  operation transformation logic;
- monitoring, usage, or stats aggregation over already-loaded stored records;
- stable shared structs/enums for admin helper results.

Representative examples:

```rust
pub fn build_admin_system_settings_payload(
    default_provider: Option<String>,
    default_model: Option<String>,
    enable_usage_tracking: bool,
    password_policy_level: String,
) -> serde_json::Value
```

Source: `crates/aether-admin/src/system.rs:655`.

```rust
pub fn build_admin_pool_batch_action_plan(
    payload: AdminPoolBatchActionRequest,
) -> Result<AdminPoolBatchActionPlan, String>
```

Source: `crates/aether-admin/src/provider/pool.rs:541`.

```rust
pub fn match_admin_monitoring_route(
    method: &http::Method,
    path: &str,
) -> Option<AdminMonitoringRoute>
```

Source: `crates/aether-admin/src/observability/monitoring.rs:997`.

## What This Crate Should Not Contain

Do not add runtime ownership here:

- no `Router::new()` or `.route(...)` declarations;
- no Axum `State<T>` or `Extension<T>` handler extractors;
- no authenticated-admin middleware;
- no SeaORM entities, `DatabaseConnection`, transactions, or migrations;
- no Redis clients or cache mutation commands;
- no tracing/logging macros;
- no background task spawning.

The current source scan found none of these patterns in `crates/aether-admin`.
When a feature needs them, implement the runtime part in the admin router or
service crate and call these helpers from there.

## Primary Design Rules

Keep helper APIs explicit. Public functions should take all required inputs as
arguments and return structured values. Avoid hidden global state.

Use domain prefixes. Good names include `admin_usage_parse_limit`,
`admin_stats_time_series_empty_response`,
`admin_provider_ops_verify_headers`, and
`build_admin_pool_batch_action_plan`.

Validate at boundaries. Query and request-body parsers distinguish missing input
from malformed input, for example `admin_usage_parse_limit` at
`crates/aether-admin/src/observability/usage.rs:58`.

Keep output deterministic. Use `BTreeSet`/`BTreeMap` where serialized admin
payloads or plans should have stable ordering, as in key-id normalization at
`crates/aether-admin/src/provider/pool.rs:562`.

Redact before presenting admin data. The usage module strips body-reference,
routing, settlement, and trace metadata at
`crates/aether-admin/src/observability/usage.rs:282` and nearby helpers.

Return errors, do not log. The caller has request id, route, user, and redaction
context. `aether-admin` should return `String`, `(StatusCode, Value)`, or
`Response<Body>`.

## Testing Expectations

Use module-local unit tests. Current examples live at:

- `crates/aether-admin/src/system.rs:2178`
- `crates/aether-admin/src/observability/usage.rs:2274`
- `crates/aether-admin/src/observability/stats.rs:2047`
- `crates/aether-admin/src/provider/ops/verify.rs:813`
- `crates/aether-admin/src/provider/status.rs:535`

Tests should cover both accepted input and rejection behavior. Prefer assertions
that lock the contract, such as status code plus error detail or normalized
output ordering, not only "returns Err".

## Tooling Notes Used To Derive This Spec

GitNexus was available for repo-level context as `repo="Aether"`. The indexed
repo reports 3,140 files, 83,229 symbols, and an Admin module cluster. Direct
GitNexus query/context calls returned a cancelled-tool response in this runtime,
so the detailed examples in these specs are grounded in direct source reads and
GitNexus repo resources.

ABCoder was requested with `repo_name="aether-admin"`, but the current Codex
runtime did not expose ABCoder MCP tools or an `abcoder` CLI. The spec therefore
uses direct Rust source evidence for symbol-level examples.

## Maintenance Checklist

When updating these guides:

- Re-read `crates/aether-admin/Cargo.toml` before claiming a dependency exists.
- Re-scan `crates/aether-admin/src/` before changing the no-database or
  no-logging guidance.
- Keep file paths and line numbers tied to real source examples.
- Remove any guide that no longer applies, and update this index immediately.
- Do not copy generic Rust advice into this directory; every rule should point
  back to a real pattern in this crate.
