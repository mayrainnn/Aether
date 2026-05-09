# Directory Structure

> Backend organization rules for `crates/aether-model-fetch`.

---

## Scope

`aether-model-fetch` is a service-layer Rust crate for model catalog discovery,
normalization, filtering, and cache-ready aggregation. It does not own an HTTP
server, SeaORM repository, Redis connection, or tokio worker. Those concerns live
in callers such as `apps/aether-gateway/src/model_fetch/runtime.rs`.

The crate exports a narrow public API from `src/lib.rs` and keeps implementation
modules private:

```rust
// crates/aether-model-fetch/src/lib.rs:1
mod association_sync;
mod config;
mod logic;
mod strategy;
mod transport;

pub use association_sync::{
    sync_provider_model_whitelist_associations, ModelFetchAssociationStore,
};
pub use config::{
    model_fetch_interval_minutes, model_fetch_startup_delay_seconds, model_fetch_startup_enabled,
};
```

GitNexus repository context for `Aether` reports this crate in a 3,140-file,
83,229-symbol codebase with service/provider execution flows. Keep this crate
small and reusable so gateway, admin query handlers, and tests can call it
without pulling application state back into the service layer.

---

## Actual Layout

```text
crates/aether-model-fetch/
├── Cargo.toml
└── src/
    ├── lib.rs              # private module declarations and public re-exports
    ├── config.rs           # environment-backed scheduler knobs
    ├── logic.rs            # pure parsing, filtering, URL, and aggregation helpers
    ├── transport.rs        # provider-specific ExecutionPlan construction
    ├── strategy.rs         # model-fetch strategy selection and execution
    └── association_sync.rs # trait-based global/provider model association sync
```

The dependency list in `Cargo.toml` is intentionally limited to sibling
contracts/transport crates plus small parsing, crypto, and async helpers:

```toml
# crates/aether-model-fetch/Cargo.toml:8
[dependencies]
aether-ai-formats.workspace = true
aether-contracts.workspace = true
aether-data-contracts.workspace = true
aether-provider-transport.workspace = true
aether-scheduler-core.workspace = true
async-trait.workspace = true
base64.workspace = true
regex.workspace = true
rsa = "0.9.10"
serde_json.workspace = true
sha2 = { workspace = true, features = ["oid"] }
uuid.workspace = true
```

Do not add application-layer dependencies from `apps/aether-gateway` or data
implementation crates here. This crate should depend on contracts and traits,
not concrete runtime state.

---

## Module Responsibilities

`lib.rs` is an API surface, not a dumping ground. Add new public functions by
placing implementation in a private module, then re-export only the stable entry
point. Existing examples are `fetch_models_from_transports`, `ModelFetchOutcome`,
and the plan builders exported from `strategy.rs` and `transport.rs`.

`config.rs` owns model-fetch environment variables. It clamps interval values
and has no caller-specific side effects:

```rust
// crates/aether-model-fetch/src/config.rs:6
pub fn model_fetch_interval_minutes() -> u64 {
    std::env::var("MODEL_FETCH_INTERVAL_MINUTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| {
            value.clamp(
                MODEL_FETCH_INTERVAL_MINUTES_MIN,
                MODEL_FETCH_INTERVAL_MINUTES_MAX,
            )
        })
        .unwrap_or(MODEL_FETCH_INTERVAL_MINUTES_DEFAULT)
}
```

`logic.rs` owns pure transformations: URL construction, response parsing,
filtering, presets, and deterministic aggregation. Keep JSON shape handling here
when it can be unit tested without a runtime.

```rust
// crates/aether-model-fetch/src/logic.rs:217
pub fn endpoint_supports_rust_models_fetch(api_format: &str) -> bool {
    let api_format = normalize_api_format(api_format);
    matches!(
        api_format.as_str(),
        "openai:chat"
            | "openai:responses"
            | "openai:responses:compact"
            | "claude:messages"
            | "gemini:generate_content"
    )
}
```

`transport.rs` turns a `GatewayProviderTransportSnapshot` into an
`ExecutionPlan`. It must not execute HTTP directly. The runtime trait method
`execute_model_fetch_execution_plan` is only declared here; execution is owned by
the application runtime implementation.

```rust
// crates/aether-model-fetch/src/transport.rs:50
#[async_trait]
pub trait ModelFetchTransportRuntime: Send + Sync {
    async fn resolve_local_oauth_request_auth(
        &self,
        transport: &GatewayProviderTransportSnapshot,
    ) -> Result<Option<LocalResolvedOAuthRequestAuth>, String>;

    async fn resolve_model_fetch_proxy(
        &self,
        transport: &GatewayProviderTransportSnapshot,
    ) -> Option<ProxySnapshot>;

    async fn execute_model_fetch_execution_plan(
        &self,
        plan: &ExecutionPlan,
    ) -> Result<ExecutionResult, String>;
}
```

`strategy.rs` selects provider behavior and coordinates page loops, fallbacks,
and metadata extraction. Provider-specific branching belongs behind
`ModelFetchStrategyKind`, not spread across callers.

```rust
// crates/aether-model-fetch/src/strategy.rs:44
pub enum ModelFetchStrategyKind {
    PresetCatalog,
    StandardTransport,
    Vertex,
    Antigravity,
    GeminiCliPreset,
    Kiro,
}
```

`association_sync.rs` owns whitelist-to-association sync, but it only speaks to
storage through `ModelFetchAssociationStore`. This is the correct place for
provider/global-model association logic; concrete database queries stay in the
caller implementing the trait.

---

## Adding Code

Put pure JSON/model transformations in `logic.rs`. Examples include
`parse_models_response_page`, `apply_model_filters`, and
`aggregate_models_for_cache`.

Put provider request shape and header construction in `transport.rs`. Examples
include `build_standard_models_fetch_execution_plan`,
`build_antigravity_fetch_available_models_plan`, and
`build_kiro_list_available_models_plan`.

Put provider selection, pagination, fallback classification, and
`ModelsFetchOutcome` assembly in `strategy.rs`. Examples include
`fetch_models_from_transports`, `fetch_vertex_models`, and
`build_success_outcome`.

Put persistence orchestration contracts in `association_sync.rs` only when the
crate needs to ask a caller for repository behavior. Do not import gateway
`AppState` here.

Put environment defaults in `config.rs`. New scheduler knobs should follow the
current `std::env::var(...).ok().and_then(...).unwrap_or(default)` pattern and
have direct unit tests.

---

## Naming Conventions

Public functions are action-oriented and domain-specific:
`fetch_models_from_transports`, `build_models_fetch_execution_plan`,
`sync_provider_model_whitelist_associations`, and
`aggregate_models_for_cache`.

Provider helpers include the provider name in the function:
`build_vertex_models_fetch_execution_plan`,
`parse_kiro_available_models_response`,
`build_antigravity_quota_payload`, and
`extract_gemini_cli_project_id`.

Types that cross the crate boundary use `ModelFetch` or `ModelsFetch` prefixes:
`ModelFetchRunSummary`, `ModelsFetchOutcome`, `ModelsFetchPage`,
`ModelFetchTransportRuntime`, and `ModelFetchAssociationStore`.

Private helpers stay private unless a caller or sibling module genuinely needs
them. For example, `build_vertex_service_account_assertion` and
`execution_result_error_message` remain private implementation details in
`strategy.rs`.

---

## Do Not

Do not add route handlers, axum extractors, or gateway state to this crate.
Gateway orchestration lives in `apps/aether-gateway/src/model_fetch/runtime.rs`,
where the worker starts and logs cycle results:

```rust
// apps/aether-gateway/src/model_fetch/runtime.rs:30
pub(crate) fn spawn_model_fetch_worker(state: AppState) -> Option<tokio::task::JoinHandle<()>> {
    if !state.has_provider_catalog_data_reader() || !state.has_provider_catalog_data_writer() {
        return None;
    }
```

Do not add direct database, Redis, or HTTP clients. Use `ModelFetchAssociationStore`
for storage-facing association sync and `ModelFetchTransportRuntime` for
execution-facing transport behavior.

Do not expose every helper from `lib.rs`. Re-export entry points and data types
that are already needed by callers; keep provider parsing details private.

Do not mix provider request construction into parser functions. A URL/header
change belongs in `transport.rs`; a JSON response shape change belongs in
`logic.rs` or `strategy.rs` parsing helpers.
