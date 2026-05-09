# Directory Structure

> Module organization rules for `crates/aether-admin`, the shared admin
> contract and helper crate.

## Scope

`aether-admin` is not the runtime admin API server. Its manifest describes it as
`Shared admin contracts and pure helpers for Aether` at
`crates/aether-admin/Cargo.toml:7`.

Keep this crate focused on reusable admin-domain code:

- request parsing and validation helpers;
- admin JSON payload builders;
- Axum-compatible `Response<Body>` helpers that do not own application state;
- provider operations specifications, normalization, and verification helpers;
- observability aggregations for usage, stats, and monitoring views.

The crate root exports only three top-level domains:

```rust
pub mod observability;
pub mod provider;
pub mod system;
```

Source: `crates/aether-admin/src/lib.rs:1`.

## Actual Layout

Current source layout:

```text
crates/aether-admin/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── system.rs
    ├── observability/
    │   ├── mod.rs
    │   ├── monitoring.rs
    │   ├── stats.rs
    │   └── usage.rs
    └── provider/
        ├── mod.rs
        ├── endpoints.rs
        ├── models.rs
        ├── models_write.rs
        ├── oauth.rs
        ├── pool.rs
        ├── quota.rs
        ├── state.rs
        ├── status.rs
        └── ops/
            ├── mod.rs
            ├── actions.rs
            ├── config.rs
            ├── verify.rs
            └── architectures/
                ├── mod.rs
                ├── anyrouter.rs
                ├── cubence.rs
                ├── generic_api.rs
                ├── nekocode.rs
                ├── new_api.rs
                ├── sub2api.rs
                └── yescode.rs
```

## Module Responsibilities

`system.rs` contains system settings, import/export, email template, module
health, adaptive key, proxy-node, and system-config helpers. It defines update
types such as `AdminSystemSettingsUpdate` at
`crates/aether-admin/src/system.rs:24` and parse/build functions such as
`parse_admin_system_settings_update` at
`crates/aether-admin/src/system.rs:669`.

`observability/usage.rs` formats request usage records, active requests, curl
commands, replay plans, aggregations, and cache/token metrics. It imports stored
audit records from `aether_data_contracts` and returns JSON values or
Axum-compatible responses, for example `admin_usage_record_json` at
`crates/aether-admin/src/observability/usage.rs:1047`.

`observability/stats.rs` owns stats-specific parsing and aggregation logic:
date ranges, time-series buckets, leaderboards, percentiles, forecasts, and
empty-response builders. Boundary parsers such as `parse_bounded_u32` live at
`crates/aether-admin/src/observability/stats.rs:142`.

`observability/monitoring.rs` owns monitoring payload helpers and a lightweight
route classifier. `AdminMonitoringRoute` is defined at
`crates/aether-admin/src/observability/monitoring.rs:21`, and
`match_admin_monitoring_route` maps method/path strings to enum variants at
`crates/aether-admin/src/observability/monitoring.rs:997`.

`provider/endpoints.rs` builds provider endpoint payloads and update records.
The read payload shape is centralized in `build_admin_provider_endpoint_response`
at `crates/aether-admin/src/provider/endpoints.rs:88`; write/update validation
uses `Result<_, String>` in `build_admin_provider_endpoint_record` at
`crates/aether-admin/src/provider/endpoints.rs:136`.

`provider/models.rs` formats stored provider models for the admin UI.
`provider/models_write.rs` normalizes and constructs model records for create,
update, import, and batch assignment. Keep write-shape constructors here rather
than duplicating them in handlers.

`provider/pool.rs` owns key-pool selection, sorting, batch actions, payloads,
and cleanup helpers. `build_admin_pool_batch_action_plan` at
`crates/aether-admin/src/provider/pool.rs:541` is the representative pattern:
parse a request-like struct, validate it, and return a plan object for the
runtime layer to execute.

`provider/quota.rs`, `provider/status.rs`, and `provider/state.rs` contain
provider/account state classification, quota parsing, OAuth nonce/PKCE helpers,
and provider identity enrichment. `AccountStatusSnapshot` is a structured
status value at `crates/aether-admin/src/provider/status.rs:57`.

`provider/ops/` is the provider operations subdomain. Its `mod.rs` file exposes
the public facade at `crates/aether-admin/src/provider/ops/mod.rs:6`, while
`architectures/` stores provider-family specs loaded by the registry in
`crates/aether-admin/src/provider/ops/architectures/mod.rs:90`.

## Public Facade Pattern

Top-level provider modules are explicitly public:

```rust
pub mod endpoints;
pub mod models;
pub mod models_write;
pub mod oauth;
pub mod ops;
pub mod pool;
pub mod quota;
pub mod state;
pub mod status;
```

Source: `crates/aether-admin/src/provider/mod.rs:1`.

Inside `provider/ops`, use a facade module for API stability. The module keeps
implementation files split by concern but re-exports the stable surface:

```rust
pub use self::actions::{
    attach_balance_checkin_outcome, parse_query_balance_payload,
    parse_sub2api_balance_payload, parse_yescode_combined_balance_payload,
    ProviderOpsCheckinOutcome,
};
```

Source: `crates/aether-admin/src/provider/ops/mod.rs:6`.

When adding provider-ops functionality, prefer a private implementation module
plus a narrow `pub use` in `provider/ops/mod.rs`. Do not make callers import
deep paths unless the deep path is intentionally part of the API.

## Naming Conventions

Use domain prefixes instead of generic helper names:

- `admin_system_*` for system settings/import/export/config helpers.
- `admin_usage_*` for usage filters, transformations, and payload builders.
- `admin_stats_*` for stats response helpers and time-series helpers.
- `admin_monitoring_*` for monitoring payloads and path helpers.
- `admin_pool_*` for provider key-pool helpers.
- `admin_provider_ops_*` for provider-ops config, headers, verification, and
  frontend credential helpers.

Good examples:

```rust
pub fn admin_usage_parse_limit(query: Option<&str>) -> Result<usize, String>
pub fn build_admin_pool_batch_action_plan(
    payload: AdminPoolBatchActionRequest,
) -> Result<AdminPoolBatchActionPlan, String>
```

Sources: `crates/aether-admin/src/observability/usage.rs:58` and
`crates/aether-admin/src/provider/pool.rs:541`.

DON'T add vague helpers such as `parse_limit`, `build_payload`, or
`normalize_config` at module scope. This crate already has several large files;
unprefixed helpers become ambiguous quickly.

## Where New Code Belongs

Add code by domain and data ownership:

- New admin system setting, config import/export field, or email-template shape:
  `system.rs`.
- New usage-list filter, usage detail field, replay/curl payload field, or usage
  aggregation: `observability/usage.rs`.
- New dashboard metric, leaderboard metric, comparison range, or time-series
  behavior: `observability/stats.rs`.
- New monitoring endpoint payload or monitoring path classifier:
  `observability/monitoring.rs`.
- New provider endpoint read/write helper: `provider/endpoints.rs`.
- New provider model write constructor: `provider/models_write.rs`.
- New provider account health/ban/quota classification: `provider/status.rs` or
  `provider/quota.rs`.
- New provider operations connector family: add a file under
  `provider/ops/architectures/`, wire it into the `LazyLock` registry, and
  expose only the needed facade symbols.

The provider-ops registry is data-driven:

```rust
static PROVIDER_OPS_ARCHITECTURES: LazyLock<Vec<ProviderOpsArchitectureSpec>> =
    LazyLock::new(|| {
        vec![
            anyrouter::spec(),
            cubence::spec(),
            generic_api::spec(),
            nekocode::spec(),
            new_api::spec(),
            sub2api::spec(),
            yescode::spec(),
        ]
    });
```

Source: `crates/aether-admin/src/provider/ops/architectures/mod.rs:90`.

## Boundaries

This crate may depend on stored value types from `aether-data` and
`aether-data-contracts`, but it should not own persistence, migrations, runtime
state injection, authentication middleware, or background task orchestration.

Evidence from the manifest:

```toml
[dependencies]
aether-data.workspace = true
aether-data-contracts.workspace = true
axum.workspace = true
serde.workspace = true
serde_json.workspace = true
```

Source: `crates/aether-admin/Cargo.toml:9`.

The `axum` dependency is used for `Json`, `Response`, `Body`, and HTTP status
helpers. It is not used here to build routers or async handlers.

DON'T add these to `aether-admin`:

```rust
Router::new()
.route("/api/admin/...", get(handler))
State<AppState>
Extension<...>
DatabaseTransaction
```

No such runtime patterns exist in the current crate scan. Keep route wiring and
database execution in the application/runtime crates that call these helpers.

## Review Checklist

Before adding or moving files in this crate:

- Does the change preserve the three top-level domains: `system`,
  `observability`, and `provider`?
- Is a new public function prefixed with its admin subdomain?
- Is the function pure with explicit inputs, or does it belong in a runtime
  crate instead?
- If it returns an Axum response, does it only shape the response and avoid
  reading application state?
- If it introduces a provider operation architecture, is it wired through
  `provider/ops/architectures/mod.rs` and re-exported through the facade only
  when necessary?
