# aether-dispatch-core Backend Spec Index

## Package Summary

`aether-dispatch-core` is a small Rust library crate for request-scoped dispatch primitives. It owns serializable domain references, dispatch-attempt sequencing, dispatch effects, and the pure pool-window cursor contract. It has no database access, no logging, no HTTP/runtime dependency, and no dependency on other Aether crates.

Cargo evidence:

```toml
// crates/aether-dispatch-core/Cargo.toml:1
[package]
name = "aether-dispatch-core"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "Request-scoped dispatch sequence and pool cursor primitives for Aether"

[dependencies]
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
tokio.workspace = true
```

Workspace evidence:

```toml
// Cargo.toml:33
[workspace.package]
edition = "2021"
```

## Public API Surface

The crate facade is `src/lib.rs`. Public consumers should import through the crate root instead of reaching into module paths.

```rust
// crates/aether-dispatch-core/src/lib.rs:6
pub use candidate::{
    DispatchCandidateRef, DispatchRankFacts, KeyRef, PoolRef, ProviderEndpointRef,
};
pub use effects::{DispatchEffect, DispatchEffectKind};
pub use pool::{
    run_pool_dispatch_cursor, PoolDispatchCursorOutcome, PoolDispatchError, PoolDispatchPort,
    PoolDispatchWindow, PoolWindowConfig, DEFAULT_POOL_MAX_SCAN, DEFAULT_POOL_PAGE_SIZE,
    DEFAULT_POOL_WINDOW_SIZE,
};
pub use sequence::{DispatchSequence, DispatchSequenceItem, DispatchSequenceMark};
```

API groups:

| Group | Re-exports | Owner |
| --- | --- | --- |
| Candidate references | `DispatchCandidateRef`, `DispatchRankFacts`, `KeyRef`, `PoolRef`, `ProviderEndpointRef` | `candidate.rs` |
| Dispatch effects | `DispatchEffect`, `DispatchEffectKind` | `effects.rs` |
| Pool cursor | `run_pool_dispatch_cursor`, `PoolDispatchCursorOutcome`, `PoolDispatchError`, `PoolDispatchPort`, `PoolDispatchWindow`, `PoolWindowConfig`, `DEFAULT_POOL_*` | `pool.rs` |
| Dispatch sequence | `DispatchSequence`, `DispatchSequenceItem`, `DispatchSequenceMark` | `sequence.rs` |

## Guidelines Index

| Guideline | File | Use it for |
| --- | --- | --- |
| Directory structure | [directory-structure.md](directory-structure.md) | File ownership, module boundaries, expansion rules |
| Error handling | [error-handling.md](error-handling.md) | `PoolDispatchError`, port error wrapping, non-error exhaustion semantics |
| Quality guidelines | [quality-guidelines.md](quality-guidelines.md) | Serde shape, `async-trait`, visibility, tests, anti-patterns |

This spec set intentionally does not include database or logging guidelines. The crate has neither DB nor logging behavior; those concerns belong in gateway/application crates.

## Known Consumers

GitNexus and source inspection show the gateway consumes this crate as a primitive contract, but does not currently implement `PoolDispatchPort` in production code.

| Consumer | Usage | Evidence |
| --- | --- | --- |
| Gateway dispatch refs | Converts `EligibleLocalExecutionCandidate` into `DispatchCandidateRef::SingleKey` or `DispatchCandidateRef::PoolRef`. | `apps/aether-gateway/src/dispatch/refs.rs:1`, `apps/aether-gateway/src/dispatch/refs.rs:7` |
| Gateway pool cursor | Uses `PoolWindowConfig::default().normalized()` and `DEFAULT_POOL_*` constants to size its production `PoolKeyCursor`. | `apps/aether-gateway/src/dispatch/pool.rs:1`, `apps/aether-gateway/src/dispatch/pool_scheduler.rs:224`, `apps/aether-gateway/src/dispatch/pool_scheduler.rs:231` |
| Gateway candidate materialization | Stores static and pending attempts as `DispatchSequence<LocalExecutionCandidateAttempt>` and creates `DispatchSequenceItem` values with `DispatchSequenceMark::Pending`. | `apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs:62`, `apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs:71`, `apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs:1193` |
| Gateway architecture test | Guards that `lib.rs` exports the core primitives. | `apps/aether-gateway/src/tests/architecture/ai_serving.rs:1340` |

## GitNexus Findings

- Repo index: `Aether`, 51,712 symbols, 111,033 relationships, 300 execution flows.
- `PoolDispatchPort` upstream impact is LOW. GitNexus found one direct implementation, the crate-local test `TestPort` in `crates/aether-dispatch-core/src/pool.rs`.
- `DispatchSequence` and `DispatchCandidateRef` have LOW graph impact from the core crate symbols, while source search shows gateway imports and uses them through the crate root.
- The dispatch-related gateway symbols live primarily in the `Dispatch`, `Planner`, and `Architecture` modules.

## Pre-Development Checklist

Before changing code in this crate:

- [ ] Confirm the change belongs in a pure domain crate, not in `apps/aether-gateway`.
- [ ] Keep runtime dependencies limited to workspace dependencies in `Cargo.toml`.
- [ ] Preserve serde's current default JSON shape for existing public types.
- [ ] Add new public primitives to `src/lib.rs`; consumers should not depend on private module paths.
- [ ] Re-check gateway usage in `apps/aether-gateway/src/dispatch/refs.rs`, `apps/aether-gateway/src/dispatch/pool.rs`, `apps/aether-gateway/src/dispatch/pool_scheduler.rs`, and `apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs`.
- [ ] If changing defaults, validate production cursor assumptions around `DEFAULT_POOL_WINDOW_SIZE`, `DEFAULT_POOL_PAGE_SIZE`, and `DEFAULT_POOL_MAX_SCAN`.
- [ ] If changing `DispatchSequence`, validate the materialization path that calls `next()` followed by `mark_succeeded()`.
- [ ] If changing `PoolDispatchPort` or `run_pool_dispatch_cursor`, update crate-local async tests first; production gateway code currently does not implement the trait.

## Quality Gates

Run the smallest relevant gate first, then downstream checks when public API or defaults change:

```bash
rtk cargo test -p aether-dispatch-core
rtk cargo check -p aether-gateway
rtk cargo test -p aether-gateway ai_serving_planner_separates_local_candidate_resolution_from_ranking
```

The architecture test is an in-crate unit test module under `aether-gateway`, not a separate integration-test target.
