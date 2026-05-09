# Directory Structure

> Backend organization rules for the `aether-ai-serving` crate.

---

## Scope

`crates/aether-ai-serving` is a service-layer Rust crate for AI serving decisions and
execution primitives. It is intentionally not an Axum handler crate and not a database
crate. The crate owns pure orchestration logic, format metadata, candidate selection,
attempt materialization, and small diagnostic payload builders. Gateway, admin, data,
transport, and persistence adapters implement the ports exposed here.

GitNexus repository context reports Aether as a multi-crate project with 83k+ symbols.
ABCoder UniAST for repo `aether-ai-serving` shows exactly one Rust module package and
25 files under `src/`. The crate dependency list is narrow: `aether-ai-formats`,
`aether-contracts`, `aether-scheduler-core`, `async-trait`, `http`, `serde`,
`serde_json`, and `url`.

---

## Actual Layout

```text
crates/aether-ai-serving/
+-- Cargo.toml
`-- src/
    +-- lib.rs
    +-- ports.rs
    +-- dto.rs
    +-- attempt_loop.rs
    +-- attempt_plan.rs
    +-- decision_input.rs
    +-- decision_path.rs
    +-- decision_payload.rs
    +-- execution_path.rs
    +-- candidate_preselection.rs
    +-- candidate_resolution.rs
    +-- candidate_ranking.rs
    +-- candidate_materialization.rs
    +-- candidate_persistence.rs
    +-- candidate_persistence_policy.rs
    +-- candidate_preparation.rs
    +-- candidate_metadata.rs
    +-- pool_scheduler.rs
    +-- ranking_metadata.rs
    +-- report_context.rs
    +-- request_body_diagnostics.rs
    +-- runtime_miss.rs
    +-- failure_diagnostic.rs
    +-- plan_payload.rs
    `-- surface_spec.rs
```

Keep this crate flat. Existing modules are file-level units, not nested directories.
Add a new `src/<concept>.rs` file when the concept is an independently testable serving
stage or metadata helper. Do not hide multiple stages in a large `service.rs` or
`utils.rs`.

---

## Public Module Surface

`src/lib.rs` declares every module and then re-exports the public API. This creates a
single crate-level import surface while keeping implementation files concept-scoped.

Example from `crates/aether-ai-serving/src/lib.rs:1`:

```rust
pub mod attempt_loop;
pub mod attempt_plan;
pub mod candidate_materialization;
pub mod candidate_metadata;
pub mod candidate_persistence;
pub mod candidate_persistence_policy;
pub mod candidate_preparation;
pub mod candidate_preselection;
pub mod candidate_ranking;
pub mod candidate_resolution;
```

The same file re-exports functions and types by concept, for example
`run_ai_candidate_resolution`, `AiCandidateResolutionPort`, and
`AiCandidateResolutionRequest` at `src/lib.rs:67`.

```rust
pub use candidate_resolution::{
    extract_ai_pool_sticky_session_token, run_ai_candidate_resolution, AiCandidateResolutionMode,
    AiCandidateResolutionOutcome, AiCandidateResolutionPort, AiCandidateResolutionRequest,
};
```

Guideline: when adding a public stage, add both `pub mod <file>;` and a focused `pub use`
block. Do not make downstream crates reach through deep module paths for normal use.

---

## Port-Oriented Stage Files

Most orchestration files follow the same structure:

1. Local outcome/request enums or structs.
2. A `Port` trait with associated types for adapter-owned data.
3. A `run_ai_*` function that executes the stage against the port.
4. Unit tests with in-file test ports.

Example from `crates/aether-ai-serving/src/candidate_resolution.rs:51`:

```rust
#[async_trait]
pub trait AiCandidateResolutionPort: Send + Sync {
    type Candidate: Send;
    type Transport: Send + Sync;
    type Eligible: Send + Sync;
    type Skipped: Send;
    type Error: Send;

    async fn read_candidate_transport(
        &self,
        candidate: &Self::Candidate,
    ) -> Result<Option<Self::Transport>, Self::Error>;
}
```

The runner is generic over the port, not over concrete gateway or data types. This is
the pattern to preserve for any new serving stage.

---

## Pipeline Modules

Use these files as the canonical pipeline ordering:

- `decision_input.rs`: reads auth/runtime facts and builds decision input.
- `decision_path.rs`: selects a local decision step for sync or stream flows.
- `execution_path.rs`: executes local steps and remote-decision fallback.
- `candidate_preselection.rs`: lists and deduplicates candidate sets by API format.
- `candidate_resolution.rs`: attaches transport, applies skip gates, ranks, then expands pools.
- `candidate_ranking.rs`: converts candidates into scheduler rankables.
- `candidate_materialization.rs`: persists or materializes attempts and skipped candidates.
- `attempt_loop.rs`: executes attempts until response, exhaustion, or no path.

Example from `crates/aether-ai-serving/src/execution_path.rs:79`:

```rust
pub async fn run_ai_sync_execution_path<Port>(
    port: &Port,
) -> Result<AiServingExecutionOutcome<Port::Response, Port::Exhaustion>, Port::Error>
where
    Port: AiSyncExecutionPathPort,
{
    let mut exhausted = None;
```

Do not mix these stages. If a gateway handler needs to read from a database, call into an
adapter that implements the port; do not add database access here.

---

## Data And Metadata Modules

The crate also has pure data helpers:

- `dto.rs` defines serialized decision and plan payloads.
- `attempt_plan.rs` converts between decisions and `aether_contracts::ExecutionPlan`.
- `surface_spec.rs` maps local format specs to serving metadata.
- `report_context.rs`, `ranking_metadata.rs`, `request_body_diagnostics.rs`,
  `runtime_miss.rs`, and `failure_diagnostic.rs` build structured diagnostics.
- `pool_scheduler.rs` owns in-crate pool grouping, skip reasons, and ordering vectors.

Example from `crates/aether-ai-serving/src/dto.rs:47`:

```rust
#[derive(Debug, Deserialize, Serialize)]
pub struct AiExecutionPlanPayload {
    pub action: String,
    #[serde(default)]
    pub plan_kind: Option<String>,
    #[serde(default)]
    pub plan: Option<ExecutionPlan>,
}
```

Guideline: payload structs in `dto.rs` are for serialized contracts. Internal stage
inputs should stay in the stage file, such as `AiCandidateResolutionRequest` in
`candidate_resolution.rs`.

---

## Naming Conventions

Names are explicit and crate-prefixed:

- Public AI-serving types start with `Ai`.
- Runner functions use `run_ai_<stage>`.
- Builders use `build_ai_*`.
- Extractors use `extract_ai_*`.
- Normalizers use `normalize_*`.
- Skip reasons are stable string constants.
- Port traits end in `Port`.
- Outcome structs/enums end in `Outcome`.

Example from `crates/aether-ai-serving/src/pool_scheduler.rs:5`:

```rust
pub const AI_POOL_ACCOUNT_BLOCKED_SKIP_REASON: &str = "pool_account_blocked";
pub const AI_POOL_ACCOUNT_EXHAUSTED_SKIP_REASON: &str = "pool_account_exhausted";
pub const AI_POOL_COOLDOWN_SKIP_REASON: &str = "pool_cooldown";
pub const AI_POOL_COST_LIMIT_REACHED_SKIP_REASON: &str = "pool_cost_limit_reached";
```

Do not introduce generic names like `Scheduler`, `Service`, `Manager`, or `Handler` in
this crate. They lose the serving-stage meaning that downstream adapters rely on.

---

## Visibility Rules

Public items are API contracts. Helper functions and helper structs stay private unless
another crate has a concrete need.

Example from `crates/aether-ai-serving/src/pool_scheduler.rs:90`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PoolGroupKey {
    provider_id: String,
    endpoint_id: String,
    model_id: String,
    selected_provider_model_name: String,
    provider_api_format: String,
    singleton_key_id: Option<String>,
}
```

`PoolGroupKey` is private because it is an implementation detail of
`run_ai_pool_scheduler`. Keep derived sort keys, score maps, and normalized preset
internals private.

---

## Where To Put New Work

Use this placement guide:

- New serving stage with async adapter calls: create `src/<stage>.rs` with a `Port` trait
  and `run_ai_<stage>` function.
- New pure mapping from one contract to another: place in `attempt_plan.rs`,
  `decision_payload.rs`, `plan_payload.rs`, or a new focused mapping file.
- New API format or local execution surface metadata: extend `surface_spec.rs`.
- New candidate skip or ordering behavior: extend `candidate_resolution.rs`,
  `candidate_ranking.rs`, or `pool_scheduler.rs`, depending on whether it gates,
  ranks, or pool-orders candidates.
- New diagnostic detail for failed request body conversion: extend
  `request_body_diagnostics.rs` and return structured extra data.

DON'T add a catch-all utility module. If the helper only exists for one stage, keep it
private in that stage file.

---

## Tests Live Beside The Stage

Every behavior-heavy module keeps unit tests in the same file under `#[cfg(test)]`.
Test ports are small in-memory adapters.

Example from `crates/aether-ai-serving/src/candidate_materialization.rs:73`:

```rust
#[derive(Default)]
struct TestPort {
    calls: Mutex<Vec<String>>,
}

#[async_trait]
impl AiCandidateMaterializationPort for TestPort {
    type Candidate = &'static str;
    type Eligible = &'static str;
    type Skipped = &'static str;
    type Attempt = &'static str;
    type Error = std::convert::Infallible;
}
```

When adding a file, add at least one test that locks the stage order or the exact
contract shape. This crate's tests favor call traces and exact outcome assertions over
mocking external services.
