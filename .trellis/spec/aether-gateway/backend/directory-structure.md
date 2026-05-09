# Directory Structure

`apps/aether-gateway` is the application crate for the Aether frontdoor. It is
organized by runtime responsibility, not by HTTP verb or database table. New
code should stay close to the existing execution boundary it extends.

## Top-Level Layout

The source root is declared in `lib.rs`; most modules are private and only a
small runtime API is exported.

```rust
// apps/aether-gateway/src/lib.rs:27
mod admin_api;
mod ai_serving;
mod api;
mod async_task;
mod audit;
mod auth;
mod cache;
mod control;
mod data;
mod error;
mod execution_runtime;
mod executor;
mod handlers;
mod maintenance;
mod scheduler;
mod state;
```

Use this mental map:

- `main.rs` owns CLI/env parsing, startup validation, database/runtime config,
  and process bootstrap.
- `router.rs` composes the axum router and global middleware.
- `control/` classifies public/admin/internal/oauth/AI routes and attaches auth
  context.
- `handlers/` owns HTTP response construction for local gateway routes.
- `ai_serving/` adapts lower-level AI serving planner/finalize primitives to
  gateway `AppState`.
- `executor/` chooses and runs local/remote/control execution paths.
- `execution_runtime/` runs execution plans and stream/sync finalization.
- `api/response.rs` builds client responses, strips/sets headers, and applies
  SSE response headers.
- `state/` is the `AppState` facade used by handlers and execution paths.
- `data/` wraps `aether_data` and exposes repository-backed read/write methods.
- `maintenance/` owns background workers and scheduled cleanup/aggregation.
- `cache/` contains local in-process caches built over `aether_cache`.
- `tests/` plus module-local `#[cfg(test)]` modules cover integration-style
  behavior, route classification, response headers, and runtime edge cases.

## Router Composition

Route mounting is centralized. Add a new first-class API family by adding a
mount in `api::*`, then mount it from `build_router_with_state`; do not add
ad-hoc route tables inside `proxy_request`.

```rust
// apps/aether-gateway/src/router.rs:27
pub fn build_router_with_state(state: AppState) -> Router {
    let cors_state = state.clone();
    let mut router = Router::<AppState>::new();
    router = api::mount_core_routes(router);
    router = api::mount_operational_routes(router);
    router = api::mount_ai_routes(router);
    router = api::mount_public_support_routes(router);
    router = api::mount_oauth_routes(router);
    router = api::mount_internal_routes(router);
    router = api::mount_admin_routes(router);
    let mut router = router
        .route("/{*path}", any(proxy_request))
        .layer(axum::middleware::from_fn(middleware::access_log_middleware))
        .with_state(state);
```

The catch-all `/{*path}` is intentional: unmatched frontdoor requests still
flow through `proxy_request` so control classification, local execution, local
errors, and execution-path headers remain consistent.

Static frontend serving is middleware, not a route family. It bypasses API,
OpenAI/Claude/Gemini, upload, gateway, and well-known paths before falling back
to `index.html`.

```rust
// apps/aether-gateway/src/router.rs:81
fn frontend_path_bypasses_static(path: &str) -> bool {
    matches!(
        path,
        "/health" | "/test-connection" | crate::constants::READYZ_PATH
    ) || path.starts_with("/api/")
        || path.starts_with("/v1/")
        || path.starts_with("/v1beta/")
        || path.starts_with("/upload/")
        || path.starts_with("/_gateway/")
        || path.starts_with("/.well-known/")
}
```

## Control Route Organization

`control/route/mod.rs` owns the route decision shape and the classification
chain. Route-family files are responsible for taxonomy; auth resolution happens
after classification.

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

The classification order is part of the contract:

```rust
// apps/aether-gateway/src/control/route/mod.rs:169
let classified = public_support::classify_public_support_route(
    method,
    &normalized_path,
    &public_models_auth_signature,
)
.or_else(|| oauth::classify_oauth_route(method, &normalized_path))
.or_else(|| admin::classify_admin_route(method, &normalized_path))
.or_else(|| internal::classify_internal_route(method, &normalized_path))
.or_else(|| ai::classify_ai_public_route(method, &normalized_path, headers))?;
```

When adding routes, add or update route-classification tests under
`src/control/tests/`. These tests assert `route_class`, `route_family`,
`route_kind`, `auth_endpoint_signature`, and `execution_runtime_candidate`.

## Handler Organization

`handlers/proxy/mod.rs` is the main frontdoor path. It is large, so new local
subflows should be extracted into files under `handlers/proxy/`, `handlers/admin/`,
`handlers/internal/`, or `handlers/public/` based on ownership.

Important current subdirectories:

- `handlers/proxy/`: catch-all request flow, local admin/internal/public handling,
  tunnel-affinity forwarding, local execution attempts, final response logging.
- `handlers/admin/`: admin domain families such as auth, billing, provider,
  observability, models, users, and feature flags.
- `handlers/internal/`: internal gateway endpoints such as decision/plan/execute
  surfaces used by control execution and runtime integration.
- `handlers/public/`: public support and AI compatibility surfaces.
- `handlers/shared/`: helper code that is shared by handler families.

Use nested modules for large admin domains. For example provider OAuth dispatch
is split by action:

```text
apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/
  batch/
  complete/
  device/
  refresh/
  start.rs
  tasks.rs
```

## State And Data Boundaries

`AppState` is the gateway facade. It owns runtime state, HTTP clients, caches,
background services, and a `GatewayDataState` reference.

```rust
// apps/aether-gateway/src/state/app.rs:44
#[derive(Debug, Clone)]
pub struct AppState {
    pub(crate) data: Arc<GatewayDataState>,
    pub(crate) runtime_state: Arc<RuntimeState>,
    pub(crate) usage_runtime: Arc<usage::UsageRuntime>,
    pub(crate) video_tasks: Arc<VideoTaskService>,
    pub(crate) request_gate: Option<Arc<ConcurrencyGate>>,
    pub(crate) distributed_request_gate: Option<Arc<RuntimeSemaphore>>,
    pub(crate) client: reqwest::Client,
```

Do not make handlers depend directly on lower-level repository types when an
`AppState` method already exists. Put domain-specific facade methods under
`state/runtime/*`, `state/catalog.rs`, `state/proxy.rs`, or `state/oauth.rs`.

`GatewayDataState` is the repository facade. It stores optional read/write
repositories and returns empty/default results when persistence is disabled.

```rust
// apps/aether-gateway/src/data/state/mod.rs:131
#[derive(Clone, Default)]
pub(crate) struct GatewayDataState {
    config: GatewayDataConfig,
    backends: Option<DataBackends>,
    auth_api_key_reader: Option<Arc<dyn AuthApiKeyReadRepository>>,
    auth_api_key_writer: Option<Arc<dyn AuthApiKeyWriteRepository>>,
    provider_catalog_reader: Option<Arc<dyn ProviderCatalogReadRepository>>,
    provider_catalog_writer: Option<Arc<dyn ProviderCatalogWriteRepository>>,
```

## Naming Conventions

- Route taxonomy fields are string labels named `route_class`, `route_family`,
  `route_kind`, and `auth_endpoint_signature`.
- Local route builders usually start with `maybe_build_*_response` when they may
  decline a route with `None`.
- Execution path functions use `maybe_execute_*` and return
  `LocalExecutionRequestOutcome`.
- Facade methods under `AppState` use domain verbs, for example
  `admin_adjust_wallet_balance`, `list_gemini_file_mappings`, or
  `cache_set_string_with_ttl`.
- Constants for externally visible headers and execution paths live in
  `constants.rs` and use upper snake case.
- Tests use behavior-oriented names such as
  `runtime_miss_detail_returns_model_specific_stream_message_when_candidates_are_unavailable`.

## DON'T

Do not create a parallel router inside `handlers/proxy/mod.rs`.

```rust
// DON'T: new catch-all branching hidden in proxy_request.
if path.starts_with("/api/new") {
    return new_handler(...).await;
}
```

Add a classifier and a mounted/local handler path instead, so access logging,
auth, response headers, and route taxonomy stay coherent.

Do not put repository SQL, Redis key construction, and response formatting in a
single handler. Use `GatewayDataState`/`AppState` for persistence and
`api::response` or handler-local response helpers for HTTP output.
