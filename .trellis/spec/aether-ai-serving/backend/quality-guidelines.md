# Quality Guidelines

> Code quality rules for `crates/aether-ai-serving`.

---

## Design Standard

This crate is a pure service-layer contract and orchestration crate. Quality is measured
by how well it keeps AI serving stages deterministic, testable, and decoupled from
gateway/data/transport details.

Keep changes small and stage-focused. A good change usually touches one of these:

- A port trait and its runner in one stage file.
- A pure DTO or contract conversion helper.
- A diagnostic/report-context helper.
- A deterministic pool or ranking rule with tests.

---

## Use Port Traits For Effects

Async effects enter through `Port` traits with associated types. Runners stay generic.

Example from `crates/aether-ai-serving/src/candidate_materialization.rs:9`:

```rust
#[async_trait]
pub trait AiCandidateMaterializationPort: Send + Sync {
    type Candidate: Send;
    type Eligible: Send + Sync;
    type Skipped: Send;
    type Attempt: Send;
    type Error: Send;

    async fn resolve_and_rank_candidates(
        &self,
        candidates: Vec<Self::Candidate>,
    ) -> Result<(Vec<Self::Eligible>, Vec<Self::Skipped>), Self::Error>;
}
```

Guideline: if a new stage must call storage, cache, scheduler, or transport code, define
a port method. Do not import concrete gateway, SeaORM, Redis, or HTTP-client types into
this crate.

---

## Preserve Deterministic Stage Order

Stage runners use explicit arrays and loops so the execution order is visible and
testable.

Example from `crates/aether-ai-serving/src/decision_path.rs:56`:

```rust
for step in [
    AiSyncDecisionStep::VideoTaskFollowUp,
    AiSyncDecisionStep::LocalVideo,
    AiSyncDecisionStep::LocalImage,
    AiSyncDecisionStep::LocalOpenAiChat,
    AiSyncDecisionStep::LocalOpenAiResponses,
    AiSyncDecisionStep::LocalStandardFamily,
    AiSyncDecisionStep::LocalSameFormatProvider,
    AiSyncDecisionStep::LocalGeminiFiles,
] {
```

When adding a new execution or decision step, update the enum, the ordered array, and
the tests that assert call order. Do not derive order from a map iteration.

---

## Normalize Inputs At Stage Boundaries

Normalize string inputs once at the stage boundary, then pass normalized values through
the rest of the function.

Example from `crates/aether-ai-serving/src/candidate_resolution.rs:117`:

```rust
let normalized_client_api_format = request.client_api_format.trim().to_ascii_lowercase();
let requested_model = request
    .requested_model
    .map(str::trim)
    .filter(|value| !value.is_empty());
```

Example from `crates/aether-ai-serving/src/attempt_plan.rs:49`:

```rust
pub fn take_ai_non_empty_string(value: &mut Option<String>) -> Option<String> {
    value.take().filter(|value| !value.trim().is_empty())
}
```

Guideline: avoid repeated ad hoc `.trim().to_ascii_lowercase()` inside nested helpers.
Normalize early and name the normalized variable.

---

## Prefer Option And Outcome Types Over Sentinel Strings

Expected absence is modeled with `Option`, `NoPath`, or skipped candidates.

Example from `crates/aether-ai-serving/src/attempt_loop.rs:60`:

```rust
let Some((last_plan, last_report_context)) = last_attempted else {
    return Ok(AiAttemptLoopOutcome::NoPath);
};
```

Example from `crates/aether-ai-serving/src/surface_spec.rs:122`:

```rust
pub fn extract_ai_gemini_model_from_path(path: &str) -> Option<String> {
    let (_, suffix) = path.split_once("/models/")?;
    let model = suffix
        .split_once(':')
        .map(|(value, _)| value)
        .unwrap_or(suffix);
```

DON'T return empty strings for "not found". Empty strings are normalized away at
boundaries.

---

## Keep Helper Types Private Until They Are Contracts

Public structs and enums are downstream contracts. Private helpers are allowed and
preferred for local ordering details.

Example from `crates/aether-ai-serving/src/pool_scheduler.rs:343`:

```rust
#[derive(Debug)]
struct PoolGroupCandidateOrdering<Candidate> {
    item: AiPoolCandidateInput<Candidate>,
    original_index: usize,
    lru_score: Option<f64>,
    cost_usage: u64,
}
```

Guideline: start helpers private. Promote them only when another crate needs to build or
inspect them directly.

---

## Use Stable Collections For Deterministic Output

This crate favors `BTreeMap` and `BTreeSet` where output ordering matters.

Example from `crates/aether-ai-serving/src/candidate_preselection.rs:56`:

```rust
let mut candidates = Vec::new();
let mut skipped_candidates = Vec::new();
let mut seen_candidates = BTreeSet::new();
let mut seen_skipped_candidates = BTreeSet::new();
```

Example from `crates/aether-ai-serving/src/pool_scheduler.rs:111`:

```rust
let mut group_order = Vec::new();
let mut groups = BTreeMap::<PoolGroupKey, Vec<AiPoolCandidateInput<Candidate>>>::new();
```

Use a separate `Vec` to preserve first-seen order when a `BTreeMap` is only used for
lookup or grouping. Do not depend on `HashMap` iteration order for scheduling decisions.

---

## Keep Scheduler Integration Narrow

`candidate_ranking.rs` adapts AI serving facts to `aether-scheduler-core` and then
applies outcomes back to candidates.

Example from `crates/aether-ai-serving/src/candidate_ranking.rs:84`:

```rust
let mut rankable =
    SchedulerRankableCandidate::from_candidate(parts.candidate, parts.original_index);
// The scheduler order is the upstream tie-breaker; AI serving only adds transport facts.
rankable.provider_id.clear();
rankable.endpoint_id.clear();
rankable.key_id.clear();
rankable.selected_provider_model_name.clear();
```

Guideline: when integrating more scheduler data, keep the reason in this adapter layer.
Do not leak scheduler internals into every candidate stage.

---

## DTOs Must Be Backward Tolerant

Serialized payload structs use `serde(default)` for optional or evolving fields. This
lets older control-plane decisions and newer execution payloads coexist.

Example from `crates/aether-ai-serving/src/dto.rs:62`:

```rust
#[derive(Debug, Deserialize, Serialize)]
pub struct AiExecutionDecision {
    pub action: String,
    #[serde(default)]
    pub decision_kind: Option<String>,
    #[serde(default)]
    pub execution_strategy: Option<String>,
    #[serde(default)]
    pub conversion_mode: Option<String>,
```

Guideline: add `#[serde(default)]` to new optional serialized fields. Avoid renaming
existing fields unless you also handle backward compatibility at the boundary.

---

## Tests Lock Behavior, Not Implementation Details

Tests are colocated and usually build a minimal test port, run the stage, and assert
both outcome and call trace.

Example from `crates/aether-ai-serving/src/execution_path.rs:324`:

```rust
#[tokio::test]
async fn sync_path_runs_scheduler_steps_before_remote_and_fallback() {
    let port = TestSyncPort {
        scheduler_supported: true,
        ..TestSyncPort::default()
    };

    let outcome = run_ai_sync_execution_path(&port).await.unwrap();
```

Example from `crates/aether-ai-serving/src/candidate_resolution.rs:330`:

```rust
#[tokio::test]
async fn resolution_reads_transport_gates_candidates_then_ranks_and_applies_pool() {
    let port = TestPort::default();

    let outcome = run_ai_candidate_resolution(
        &port,
        vec!["first", "missing", "inactive", "unsupported", "second"],
        AiCandidateResolutionRequest::standard(" OpenAI:Chat ", Some(" gpt-4.1 ")),
    )
    .await
    .unwrap();
```

For async runners, use `#[tokio::test]`. For pure helpers, use `#[test]`.

---

## Anti-Patterns

DON'T add new dependencies unless the crate boundary truly needs them. Current
production dependencies are all workspace dependencies and no database/logging/error
framework is present.

DON'T add a concrete adapter implementation here. If you find yourself importing an
entity model, Redis client, Axum extractor, or HTTP client, the code belongs in another
crate.

DON'T use `HashMap` or random ordering for pool scheduling, candidate selection, or
report field order when deterministic tests can cover the behavior.

DON'T use `unwrap` or `expect` in production code for external data. Existing production
`expect` calls document internal invariants such as an already-created group in
`pool_scheduler.rs:139` or serializing control-owned maps in `report_context.rs:126`.

DON'T add broad "helper" files. The existing naming is stage-specific and makes public
API review possible from `lib.rs`.

---

## Review Checklist For This Crate

- Does the change keep the effectful boundary behind a `Port` trait?
- Is the runner generic over adapter-owned candidate/transport/response types?
- Are skip and miss reasons stable, machine-readable strings?
- Are serialized DTO additions `#[serde(default)]` where appropriate?
- Is the execution order explicit and covered by a unit test?
- Are report/diagnostic facts returned as structured data rather than logged?
- Does `src/lib.rs` expose only the intended public API?
