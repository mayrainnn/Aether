# Backend Development Guidelines

> Entry point for `crates/aether-model-fetch`.

---

## What This Crate Does

`aether-model-fetch` is the service-layer crate that fetches, parses, filters,
normalizes, and aggregates upstream provider model catalogs. It is used by
gateway scheduled workers and admin/provider query paths, but it deliberately
does not own gateway runtime state.

The public API is exported from `src/lib.rs`:

```rust
// crates/aether-model-fetch/src/lib.rs:20
pub use strategy::{
    fetch_models_from_transports, ModelFetchStrategy, ModelFetchStrategyKind, ModelsFetchOutcome,
    SelectedModelFetchStrategy,
};
pub use transport::{
    build_antigravity_fetch_available_models_plan, build_gemini_cli_load_code_assist_plan,
    build_kiro_list_available_models_plan, build_models_fetch_execution_plan,
    build_standard_models_fetch_execution_plan, build_vertex_models_fetch_execution_plan,
    ModelFetchTransportRuntime,
};
```

GitNexus MCP context for repo `Aether` reports a large multi-layer Rust codebase
with 3,140 files, 83,229 symbols, and 300 execution flows. The repo clusters
include `Services`, `Provider`, `Runtime`, `Admin`, and `Frontdoor`, which
matches this crate's position as a service/provider boundary rather than an app
or database implementation.

ABCoder MCP was requested for AST-level inspection with
`repo_name="aether-model-fetch"`, but the current Codex session did not expose
an ABCoder MCP namespace, and no `abcoder` CLI was available on PATH. The specs
below are therefore grounded in GitNexus MCP resources plus direct source reads
from the crate and its gateway integration points.

---

## Read These Guides

| Guide | Purpose |
|-------|---------|
| [Directory Structure](./directory-structure.md) | Module boundaries, file responsibilities, and where new code belongs. |
| [Error Handling](./error-handling.md) | `Result<T, String>`, partial outcomes, trait-associated storage errors, and caller mapping. |
| [Quality Guidelines](./quality-guidelines.md) | Deterministic normalization, provider strategy rules, test expectations, and forbidden dependencies. |
| [Logging Guidelines](./logging-guidelines.md) | Why this crate currently does not log directly, and how callers log model-fetch events. |

`database-guidelines.md` was removed for this crate. `aether-model-fetch` does
not directly use SeaORM, Redis, migrations, database connections, or repository
implementations. It only defines storage-facing traits and consumes contract
types. Concrete persistence lives in gateway state and data crates.

---

## Pre-Development Checklist

Before changing this crate:

1. Read [Directory Structure](./directory-structure.md) to place code in the
   correct module.
2. Read [Error Handling](./error-handling.md) before adding new provider
   failures or partial-success paths.
3. Read [Quality Guidelines](./quality-guidelines.md) before changing parser
   output, aggregation order, public re-exports, or strategy selection.
4. Read [Logging Guidelines](./logging-guidelines.md) before adding any tracing
   or diagnostic output.
5. Check the gateway integration call sites if the change affects scheduled
   workers, admin provider query, upstream cache writes, or whitelist sync.

Key integration paths:

```rust
// apps/aether-gateway/src/model_fetch/runtime.rs:8
use aether_model_fetch::{
    apply_model_filters, fetch_models_from_transports, json_string_list, merge_upstream_metadata,
    model_fetch_interval_minutes, model_fetch_startup_delay_seconds, model_fetch_startup_enabled,
    preset_models_for_provider, selected_models_fetch_endpoints,
    sync_provider_model_whitelist_associations, ModelFetchAssociationStore, ModelFetchRunSummary,
};
```

```rust
// apps/aether-gateway/src/handlers/admin/provider/query/models.rs:1707
let outcome = match fetch_models_from_transports(state.app(), &transports).await {
    Ok(outcome) => outcome,
    Err(err) => {
        all_errors.push(err);
        if let Some(fallback) = provider_query_codex_preset_fallback(provider) {
            return Ok(fallback);
        }
```

---

## Architectural Rules

Keep source modules private. Re-export only stable, caller-needed entry points
from `lib.rs`.

Keep direct provider execution abstract. The crate builds `ExecutionPlan` values
and calls `ModelFetchTransportRuntime`; it does not own HTTP clients.

Keep storage abstract. The crate defines `ModelFetchAssociationStore`; gateway
`AppState` implements it.

Keep provider model outputs deterministic. Use `BTreeMap` and `BTreeSet` when
deduplication or order affects cached JSON, UI output, or tests.

Keep errors sanitized and caller-loggable. Return short strings or
`ModelsFetchOutcome.errors`; do not log secrets or dump raw headers.

Keep tests local to the module. Use fake runtimes and JSON fixtures, not real
network calls.

---

## Quality Check

Run targeted crate verification after edits:

```bash
cargo test -p aether-model-fetch
```

If you changed gateway integration behavior, also run the relevant gateway tests
that cover model-fetch runtime or admin provider query flows.

Check for leftover template text in this spec directory:

```bash
rg -n "template placeholder" .trellis/spec/aether-model-fetch/backend
```

Check that database guidance is not reintroduced unless the crate directly
starts owning database connections or queries. Trait-based storage calls alone
do not make this a database crate.

---

## Common Change Paths

Adding a new OpenAI-like model response shape usually belongs in `logic.rs`:
adjust `model_id_from_openai_like_item`, `parse_models_response_page`, or
`normalize_cached_model`, then add tests under `logic.rs`.

Adding a provider-specific request path usually spans `transport.rs` and
`strategy.rs`: build the plan in `transport.rs`, choose or execute the strategy
in `strategy.rs`, then add fake-runtime async tests.

Adding a provider preset belongs in `preset_models_for_provider` and must have a
test asserting the full ID list if the order matters.

Adding whitelist/association behavior belongs in `association_sync.rs` only if
it can be expressed through `ModelFetchAssociationStore`. If it needs a concrete
repository, put that in the gateway/data implementation instead.

---

## Do Not

Do not add `database-guidelines.md` back just to document caller persistence.
Document direct database patterns in the caller crate that owns the queries.

Do not import `AppState`, `GatewayError`, axum handlers, SeaORM entities, Redis
clients, or HTTP clients into this crate.

Do not add placeholder docs. Every guide in this directory should cite real
source paths and current behavior.

Do not treat GitNexus or ABCoder availability as a reason to invent generic
guidance. If code intelligence is unavailable, read source files and say exactly
what evidence was used.
