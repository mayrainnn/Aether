# Aether Scheduler Core Backend Guidelines

> Project-specific coding guidance for `crates/aether-scheduler-core`.

## Crate Summary

`aether-scheduler-core` is the pure scheduler domain crate for Aether provider
selection. It evaluates candidate rows, auth constraints, model mappings,
provider quota, provider-key health, RPM limits, affinity, ranking, and
request-candidate report metadata.

The crate description is explicit:

```toml
description = "Pure scheduler health and quota logic extracted from aether-gateway"
```

Source: `crates/aether-scheduler-core/Cargo.toml:7`.

Its root is a private-module facade with public re-exports:

```rust
mod affinity;
mod auth;
mod candidate;
mod health;
mod model;
mod provider;
mod ranking;
mod request_candidate;
```

Source: `crates/aether-scheduler-core/src/lib.rs:1`.

## Final Guide Set

| Guide | Status | Purpose |
| --- | --- | --- |
| [Directory Structure](./directory-structure.md) | Complete | Documents the real module tree, private-module facade, domain ownership, and where new scheduling helpers belong. |
| [Error Handling](./error-handling.md) | Complete | Captures the crate's `DataLayerError`, `Option`, bool, skip-reason, and request-candidate error-shaping conventions. |
| [Quality Guidelines](./quality-guidelines.md) | Complete | Covers pure domain boundaries, visibility, input structs, deterministic ordering, JSON parsing, numeric safety, and tests. |
| [Logging Guidelines](./logging-guidelines.md) | Complete | Documents the current no-logging boundary and the structured values callers should log instead. |
| [Scheduler Domain Patterns](./scheduler-domain-patterns.md) | Complete | Explains candidate enumeration, runtime selectability, health/RPM, ranking, affinity, and request-candidate reporting patterns. |

`database-guidelines.md` was intentionally removed. This crate does not use
SeaORM, Redis, transactions, migrations, connection pools, SQL, or repositories.
It consumes stored record structs from `aether-data-contracts`, but persistence
belongs to caller crates.

## Core Public APIs

Candidate enumeration converts stored minimal selection rows into scheduler
candidates:

```rust
pub fn enumerate_minimal_candidate_selection(
    input: EnumerateMinimalCandidateSelectionInput<'_>,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, DataLayerError>
```

Source: `crates/aether-scheduler-core/src/candidate/enumeration.rs:10`.

Runtime selectability reports a stable skip reason:

```rust
pub fn candidate_runtime_skip_reason_with_state(
    input: CandidateRuntimeSelectabilityInput<'_>,
) -> Option<&'static str>
```

Source: `crates/aether-scheduler-core/src/candidate/selectability.rs:41`.

Ranking reorders caller-owned item slices and returns ranking metadata:

```rust
pub fn apply_scheduler_candidate_ranking<T>(
    items: &mut [T],
    candidates: &[SchedulerRankableCandidate],
    context: SchedulerRankingContext,
) -> Vec<SchedulerRankingOutcome>
```

Source: `crates/aether-scheduler-core/src/ranking/mod.rs:66`.

Request-candidate reporting creates persistence payloads without owning the
repository:

```rust
pub fn build_execution_request_candidate_seed(
    plan: &ExecutionPlan,
    report_context: Option<&Value>,
    started_at_unix_ms: u64,
    generated_candidate_id: String,
) -> SchedulerExecutionRequestCandidateSeed
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:308`.

## What This Crate Should Contain

Add code here when it is pure scheduler domain logic over already-loaded data:

- provider, API-format, and model auth constraints;
- model name resolution and provider model mapping;
- candidate enumeration and capability priority;
- runtime selectability over recent request candidates and provider-key state;
- provider quota and provider concurrency helper maps;
- health bucket, cooldown, circuit breaker, adaptive RPM, and reservation math;
- candidate ranking, affinity, load-balance rotation, and ranking explanations;
- request-candidate seed/status/report-context payload construction.

## What This Crate Should Not Contain

Do not add:

- Axum routes, extractors, middleware, or `Response` builders;
- SeaORM entities, `DatabaseConnection`, `ConnectionTrait`, migrations, SQL, or
  repository traits;
- Redis/cache clients or cache mutation commands;
- `tokio::spawn`, timers, background workers, or runtime state ownership;
- `tracing`, `println!`, `dbg!`, or request-level logging;
- provider transport network calls or OAuth refresh behavior.

Those concerns belong in gateway, services, data, provider-transport, runtime
state, or another owner crate.

## Primary Design Rules

Keep APIs explicit. Wide decision inputs should use input structs with borrowed
fields, like `EnumerateMinimalCandidateSelectionInput` at
`crates/aether-scheduler-core/src/candidate/types.rs:31`.

Keep output deterministic. Ranking tie-breakers end in identity and
`original_index` at `crates/aether-scheduler-core/src/ranking/modes.rs:27`, and
model-name collection uses `BTreeSet` at
`crates/aether-scheduler-core/src/candidate/enumeration.rs:117`.

Preserve machine-readable reason strings. Existing ranking reasons live in
`crates/aether-scheduler-core/src/ranking/reasons.rs:5`; runtime skip reasons
live in `crates/aether-scheduler-core/src/candidate/selectability.rs:56`.

Validate persisted JSON loudly when the schema is wrong. The strict
`DataLayerError::UnexpectedValue` path in
`crates/aether-scheduler-core/src/model.rs:316` protects priority semantics from
silent drift.

Return values instead of logging. Scheduler-core returns ranking outcomes, skip
reasons, health buckets, and extra data; caller crates log them with request and
redaction context.

## Tooling Evidence Used

GitNexus was used with `repo="Aether"` through the repo resource and CLI
fallback. The repo context reports 3,140 files, 83,229 symbols, and 300
execution flows. A Cypher query over `crates/aether-scheduler-core` confirmed
the indexed files, enums, and functions. GitNexus context showed
`apply_scheduler_candidate_ranking` is called by gateway ranking paths and
`crates/aether-ai-serving/src/candidate_ranking.rs`.

ABCoder MCP was used with `repo_name="aether-scheduler-core"` by starting the
configured stdio MCP server against the scheduler AST file. `list_repos`,
`get_repo_structure`, `get_file_structure`, and `get_ast_node` confirmed the
18-file Rust module, function signatures, dependencies, and code for key nodes
such as `apply_scheduler_candidate_ranking`,
`enumerate_minimal_candidate_selection`,
`extract_global_priority_for_format`, `provider_key_rpm_allows_request_since`,
`build_execution_request_candidate_seed`, and `SchedulerRankableCandidate`.

## Maintenance Checklist

When updating these guides:

- Re-read `crates/aether-scheduler-core/Cargo.toml` before claiming a dependency
  or runtime concern exists.
- Re-scan `crates/aether-scheduler-core/src` before changing the no-database or
  no-logging guidance.
- Keep line-number examples tied to real code, not desired future design.
- Check GitNexus context for exported functions before changing public API
  guidance, especially ranking and candidate enumeration.
- Use ABCoder `get_file_structure`/`get_ast_node` for exact signatures when
  adding new code examples.
