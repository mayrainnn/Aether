# Directory Structure

## Crate Layout

```text
crates/aether-dispatch-core/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── candidate.rs
    ├── effects.rs
    ├── pool.rs
    └── sequence.rs
```

This crate uses a flat module layout. Do not introduce nested `mod.rs` trees unless a new domain area grows beyond a single focused file.

## Module Ownership

### `lib.rs`

`lib.rs` is the API facade. It declares the four modules and re-exports all supported public primitives.

```rust
// crates/aether-dispatch-core/src/lib.rs:1
pub mod candidate;
pub mod effects;
pub mod pool;
pub mod sequence;
```

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

Rules:

- Add new supported public types to `lib.rs`.
- Keep `lib.rs` free of behavior and helper functions.
- Consumers should prefer `aether_dispatch_core::TypeName` over `aether_dispatch_core::module::TypeName`.

### `candidate.rs`

`candidate.rs` owns lightweight references to dispatchable upstream choices. These are logical references, not database rows and not transport clients.

```rust
// crates/aether-dispatch-core/src/candidate.rs:1
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderEndpointRef {
    pub provider_id: String,
    pub endpoint_id: String,
    pub model_id: String,
    pub selected_provider_model_name: String,
    pub api_format: String,
}
```

```rust
// crates/aether-dispatch-core/src/candidate.rs:37
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DispatchCandidateRef {
    SingleKey {
        key: KeyRef,
        rank: DispatchRankFacts,
    },
    PoolRef {
        pool: PoolRef,
        rank: DispatchRankFacts,
    },
}
```

Gateway conversion lives outside this crate:

```rust
// apps/aether-gateway/src/dispatch/refs.rs:7
pub(crate) fn dispatch_ref_for_local_candidate(
    eligible: &EligibleLocalExecutionCandidate,
) -> DispatchCandidateRef {
    let rank = DispatchRankFacts {
        provider_priority: eligible.candidate.provider_priority,
        key_priority: Some(eligible.candidate.key_internal_priority),
        ranking_reason: eligible.ranking.as_ref().and_then(|ranking| {
            ranking
                .promoted_by
                .or(ranking.demoted_by)
                .map(str::to_string)
        }),
    };
```

Ownership rule: keep gateway-only details such as `EligibleLocalExecutionCandidate`, `LocalExecutionCandidateKind`, and fallback group-id formatting in gateway. Core refs should stay transport- and storage-agnostic.

### `effects.rs`

`effects.rs` owns the serializable dispatch-effect data model.

```rust
// crates/aether-dispatch-core/src/effects.rs:1
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DispatchEffectKind {
    CandidateFailed,
    RateLimited,
    Succeeded,
}
```

```rust
// crates/aether-dispatch-core/src/effects.rs:8
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DispatchEffect {
    pub kind: DispatchEffectKind,
    pub provider_id: String,
    pub endpoint_id: String,
    pub key_id: Option<String>,
    pub candidate_index: u32,
    pub reason: Option<String>,
}
```

Ownership rule: this file defines effect facts only. It must not write audit records, metrics, logs, cache entries, or database rows.

### `pool.rs`

`pool.rs` owns reusable pool-window sizing and the pure async cursor contract.

```rust
// crates/aether-dispatch-core/src/pool.rs:3
pub const DEFAULT_POOL_WINDOW_SIZE: u32 = 16;
pub const DEFAULT_POOL_PAGE_SIZE: u32 = 64;
pub const DEFAULT_POOL_MAX_SCAN: u32 = 512;
```

```rust
// crates/aether-dispatch-core/src/pool.rs:24
impl PoolWindowConfig {
    pub fn normalized(self) -> Self {
        let page_size = self.page_size.max(1);
        let window_size = self.window_size.max(1).min(page_size);
        let max_scan = self.max_scan.max(window_size);
        Self {
            window_size,
            page_size,
            max_scan,
        }
    }
}
```

```rust
// crates/aether-dispatch-core/src/pool.rs:56
#[async_trait]
pub trait PoolDispatchPort {
    type Candidate: Send;
    type Error: Send;

    async fn read_page(
        &mut self,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Self::Candidate>, Self::Error>;

    async fn rank_and_filter_window(
        &mut self,
        candidates: Vec<Self::Candidate>,
        window_size: u32,
    ) -> Result<PoolDispatchWindow<Self::Candidate>, Self::Error>;
}
```

Production gateway code currently consumes the defaults but keeps its runtime cursor in gateway:

```rust
// apps/aether-gateway/src/dispatch/pool_scheduler.rs:231
let window_config = crate::dispatch::pool::default_pool_window_config().normalized();
let max_scanned_keys = max_scanned_keys.min(window_config.max_scan);
```

Ownership rule: keep concrete page loading, runtime state reads, cooldown checks, logging, and skipped-candidate diagnostics in gateway. `aether-dispatch-core` owns only reusable config and pure cursor abstractions.

### `sequence.rs`

`sequence.rs` owns ordered dispatch attempts with a local cursor and per-item mark.

```rust
// crates/aether-dispatch-core/src/sequence.rs:16
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchSequence<Candidate> {
    items: Vec<DispatchSequenceItem<Candidate>>,
    cursor: usize,
}
```

```rust
// crates/aether-dispatch-core/src/sequence.rs:45
#[allow(clippy::should_implement_trait)]
pub fn next(&mut self) -> Option<&DispatchSequenceItem<Candidate>> {
    while self
        .items
        .get(self.cursor)
        .is_some_and(|item| item.mark != DispatchSequenceMark::Pending)
    {
        self.cursor = self.cursor.saturating_add(1);
    }
    self.items.get(self.cursor)
}
```

Gateway uses the sequence to preserve ordered attempts:

```rust
// apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs:1193
fn dispatch_sequence_from_attempts(
    attempts: Vec<LocalExecutionCandidateAttempt>,
) -> DispatchSequence<LocalExecutionCandidateAttempt> {
    DispatchSequence::new(
        attempts
            .into_iter()
            .map(|attempt| DispatchSequenceItem {
                candidate_index: attempt.candidate_index,
                retry_index: attempt.retry_index,
                candidate: attempt,
                mark: aether_dispatch_core::DispatchSequenceMark::Pending,
            })
            .collect(),
    )
}
```

Ownership rule: keep sequencing generic over `Candidate`. Do not add gateway-specific attempt fields directly to core sequence types.

## Expansion Rules

Add a new source file only when the new concept is a stable dispatch-domain primitive with its own public API surface. Good candidates are new reference/effect/sequence/pool concepts. Poor candidates are gateway policy, database reads, runtime diagnostics, or transport-specific behavior.

When adding a module:

1. Create `src/<domain>.rs`.
2. Add `pub mod <domain>;` to `src/lib.rs`.
3. Re-export public primitives from `src/lib.rs`.
4. Add focused unit tests beside the implementation.
5. Update `apps/aether-gateway/src/tests/architecture/ai_serving.rs` if the new primitive is part of the architectural contract.
6. Update this spec index.

Do not split files only for size. Current files are intentionally compact and cohesive.

## Anti-Patterns

- Do not move gateway runtime cursor behavior from `apps/aether-gateway/src/dispatch/pool_scheduler.rs` into this crate.
- Do not add DB repositories, HTTP clients, cache access, tracing, or task spawning here.
- Do not introduce nested directories for one or two types.
- Do not add module-local public APIs that are not re-exported from `lib.rs`.
- Do not make the core crate depend on gateway, scheduler, provider, data-contract, or admin crates.
