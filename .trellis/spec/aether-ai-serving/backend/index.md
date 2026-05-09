# aether-ai-serving Backend Guidelines

> Entry point for AI serving service-layer guidance.

---

## Package

- Package: `aether-ai-serving`
- Path: `crates/aether-ai-serving`
- Layer: service-layer contracts and orchestration
- Language: Rust
- Runtime assumptions: async traits via `async-trait`; async execution tested with Tokio
- Main internal dependencies: `aether-ai-formats`, `aether-contracts`,
  `aether-scheduler-core`
- Non-goals: Axum handlers, SeaORM queries, Redis access, HTTP transport execution,
  tracing setup, and concrete gateway adapters

This crate owns the reusable AI serving pipeline pieces: decision input, local decision
paths, execution paths, candidate preselection/resolution/ranking/materialization,
attempt loops, plan payload conversion, surface metadata, pool scheduling, and structured
diagnostics.

---

## Pre-Development Checklist

Read these files before editing `crates/aether-ai-serving`:

1. [Directory Structure](./directory-structure.md)
2. [Quality Guidelines](./quality-guidelines.md)
3. [Error Handling](./error-handling.md)
4. [Logging Guidelines](./logging-guidelines.md)

Then inspect the source file for the specific stage you are modifying. The public API is
declared from `crates/aether-ai-serving/src/lib.rs`, so review its `pub use` block before
changing public names.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Actual module layout, stage placement, naming, and public API rules | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Port traits, deterministic ordering, DTO compatibility, testing, and anti-patterns | Filled |
| [Error Handling](./error-handling.md) | Generic port errors, domain outcomes, skip reasons, and structured diagnostics | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Logger-free crate boundary, report context, diagnostic payloads, and sensitive data rules | Filled |

`database-guidelines.md` was intentionally removed. `aether-ai-serving` has no SeaORM,
Redis, SQLx, entity, transaction, or connection dependency, and source scans show no
database access. Database and cache behavior belongs in adapter crates that implement the
ports exposed here.

---

## Core Development Rules

1. Keep effects behind `Port` traits.
2. Keep runner functions generic over adapter-owned associated types.
3. Use explicit outcome enums, `Option`, skipped candidates, and stable reason strings
   for expected domain misses.
4. Propagate adapter failures with `?`; do not invent a crate-wide error enum.
5. Keep scheduling, ranking, and selection deterministic.
6. Use `BTreeMap`, `BTreeSet`, and first-seen `Vec` ordering when output order matters.
7. Add or update in-file unit tests for every behavior change.
8. Update `src/lib.rs` re-exports when adding public API.
9. Do not add tracing or database dependencies to this crate.
10. Do not add catch-all `utils`, `service`, or `manager` modules.

---

## Canonical Code Examples

Port trait pattern from `crates/aether-ai-serving/src/candidate_resolution.rs:51`:

```rust
#[async_trait]
pub trait AiCandidateResolutionPort: Send + Sync {
    type Candidate: Send;
    type Transport: Send + Sync;
    type Eligible: Send + Sync;
    type Skipped: Send;
    type Error: Send;
}
```

Execution outcome pattern from `crates/aether-ai-serving/src/execution_path.rs:3`:

```rust
#[derive(Debug)]
pub enum AiServingExecutionOutcome<Response, Exhaustion> {
    Responded(Response),
    Exhausted(Exhaustion),
    NoPath,
}
```

Deterministic pool scheduling entry point from
`crates/aether-ai-serving/src/pool_scheduler.rs:106`:

```rust
pub fn run_ai_pool_scheduler<Candidate>(
    candidates: Vec<AiPoolCandidateInput<Candidate>>,
    runtime_by_provider: &BTreeMap<String, AiPoolRuntimeState>,
    load_balance_seed_nonce: &str,
) -> AiPoolSchedulerOutcome<Candidate> {
```

DTO compatibility pattern from `crates/aether-ai-serving/src/dto.rs:62`:

```rust
#[derive(Debug, Deserialize, Serialize)]
pub struct AiExecutionDecision {
    pub action: String,
    #[serde(default)]
    pub decision_kind: Option<String>,
    #[serde(default)]
    pub execution_strategy: Option<String>,
}
```

---

## Quality Check

Before reporting a change to this crate as complete:

- Run a targeted test command for the crate, normally `cargo test -p aether-ai-serving`.
- If only docs changed, run at least static checks for this spec directory:
  placeholder search, HTML comment search, and file line counts.
- Confirm no deleted guideline is still referenced from this `index.md`.
- Confirm examples include real file paths and line anchors.
- Confirm any new public item is re-exported intentionally from `src/lib.rs`.

---

## Tooling Evidence Used For These Specs

These guidelines were filled from:

- GitNexus MCP resources for repo `Aether`: repository context, clusters, process list,
  and the `Proxy_request -> Is_execution_runtime_candidate` process trace.
- ABCoder UniAST artifact for repo `aether-ai-serving` at
  `/Users/mayrain/abcoder-asts/aether-ai-serving-ast.json`, which lists the one Rust
  module package, all 25 source files, and symbol dependencies for key runner functions.
- Direct source reads from `crates/aether-ai-serving/src/*.rs`.

The configured Codex MCP list shows an `abcoder` MCP server, but this Codex tool surface
did not expose callable ABCoder functions. Use `repo_name="aether-ai-serving"` when those
tools are available in another session.
