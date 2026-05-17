# Quality Guidelines

## Serialization Contracts

Public data types use serde derives and serde's default JSON representation. That means struct fields remain snake_case and enum variants use external tagging unless a type explicitly opts into another representation.

Candidate refs:

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

Effect data:

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

Sequence items:

```rust
// crates/aether-dispatch-core/src/sequence.rs:1
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DispatchSequenceItem<Candidate> {
    pub candidate_index: u32,
    pub retry_index: u32,
    pub candidate: Candidate,
    pub mark: DispatchSequenceMark,
}
```

Expected JSON shape examples:

```json
{
  "provider_id": "provider-1",
  "endpoint_id": "endpoint-1",
  "model_id": "model-1",
  "selected_provider_model_name": "gpt-5",
  "api_format": "openai:chat"
}
```

```json
{
  "SingleKey": {
    "key": {
      "provider_id": "provider-1",
      "endpoint_id": "endpoint-1",
      "key_id": "key-1",
      "model_id": "model-1",
      "selected_provider_model_name": "gpt-5",
      "api_format": "openai:chat"
    },
    "rank": {
      "provider_priority": 10,
      "key_priority": 7,
      "ranking_reason": "sticky_session"
    }
  }
}
```

Rules:

- Do not add `#[serde(rename_all = "...")]` to existing public types.
- Do not switch `DispatchCandidateRef` to internally or adjacently tagged enum encoding.
- Add serde tests if changing any serialized field, enum variant, or generic bound.
- Keep generic serialized types like `DispatchSequenceItem<Candidate>` generic; the caller's `Candidate` type must provide serde impls when serialization is needed.

## Defaults and Normalization

`DispatchRankFacts` is the only domain fact type with a derived default:

```rust
// crates/aether-dispatch-core/src/candidate.rs:30
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DispatchRankFacts {
    pub provider_priority: i32,
    pub key_priority: Option<i32>,
    pub ranking_reason: Option<String>,
}
```

`PoolWindowConfig` uses explicit constants:

```rust
// crates/aether-dispatch-core/src/pool.rs:3
pub const DEFAULT_POOL_WINDOW_SIZE: u32 = 16;
pub const DEFAULT_POOL_PAGE_SIZE: u32 = 64;
pub const DEFAULT_POOL_MAX_SCAN: u32 = 512;
```

```rust
// crates/aether-dispatch-core/src/pool.rs:14
impl Default for PoolWindowConfig {
    fn default() -> Self {
        Self {
            window_size: DEFAULT_POOL_WINDOW_SIZE,
            page_size: DEFAULT_POOL_PAGE_SIZE,
            max_scan: DEFAULT_POOL_MAX_SCAN,
        }
    }
}
```

Normalization is fail-soft and keeps invalid caller input out of error handling:

```rust
// crates/aether-dispatch-core/src/pool.rs:24
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
```

Gateway depends on these defaults:

```rust
// apps/aether-gateway/src/dispatch/pool_scheduler.rs:224
.unwrap_or(u64::from(aether_dispatch_core::DEFAULT_POOL_PAGE_SIZE))
```

```rust
// apps/aether-gateway/src/dispatch/pool_scheduler.rs:231
let window_config = crate::dispatch::pool::default_pool_window_config().normalized();
```

## `async-trait` Usage

`PoolDispatchPort` is the only async trait in the crate.

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

Trait rules:

- Use associated types for `Candidate` and `Error`.
- Keep bounds minimal: `Send` only.
- Keep `&mut self`; ports may track pages, caches, or test-observed limits.
- Keep the two-phase protocol: `read_page` fetches, `rank_and_filter_window` trims and ranks.
- Keep port implementations outside this crate except for tests.

The current crate-local test port shows the intended shape:

```rust
// crates/aether-dispatch-core/src/pool.rs:137
#[async_trait]
impl PoolDispatchPort for TestPort {
    type Candidate = u32;
    type Error = std::convert::Infallible;

    async fn read_page(
        &mut self,
        _offset: u32,
        limit: u32,
    ) -> Result<Vec<Self::Candidate>, Self::Error> {
        self.read_limits.push(limit);
        Ok(self.pages.pop_front().unwrap_or_default())
    }

    async fn rank_and_filter_window(
        &mut self,
        mut candidates: Vec<Self::Candidate>,
        window_size: u32,
    ) -> Result<PoolDispatchWindow<Self::Candidate>, Self::Error> {
        candidates.sort();
        candidates.truncate(window_size as usize);
        let scanned_count = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        Ok(PoolDispatchWindow {
            candidates,
            scanned_count,
        })
    }
}
```

## Visibility Rules

Use `pub` for crate-root primitives and private `fn` for implementation helpers.

Examples:

```rust
// crates/aether-dispatch-core/src/sequence.rs:81
fn mark_current(
    &mut self,
    mark: DispatchSequenceMark,
) -> Option<&DispatchSequenceItem<Candidate>> {
    let item = self.items.get_mut(self.cursor)?;
    item.mark = mark;
    self.cursor = self.cursor.saturating_add(1);
    self.items.get(self.cursor)
}
```

```rust
// apps/aether-gateway/src/dispatch/refs.rs:71
fn pool_group_id_for_provider_endpoint(eligible: &EligibleLocalExecutionCandidate) -> String {
    format!(
        "provider={}|endpoint={}|model={}|selected_model={}|api_format={}",
        eligible.candidate.provider_id,
        eligible.candidate.endpoint_id,
        eligible.candidate.model_id,
        eligible.candidate.selected_provider_model_name,
        eligible.candidate.endpoint_api_format
    )
}
```

Rules:

- Avoid `pub(crate)` in `aether-dispatch-core`; there is no internal multi-module service layer here.
- Keep fields public only on pure data structs where external construction is part of the contract.
- Keep `DispatchSequence` internals private so cursor movement remains controlled by methods.
- Re-export every public primitive from `lib.rs`.

## Arithmetic and Cursor Safety

Use saturating arithmetic and checked conversions where runtime counts cross type boundaries.

```rust
// crates/aether-dispatch-core/src/pool.rs:99
let page_len = u32::try_from(page.len()).unwrap_or(u32::MAX);
offset = offset.saturating_add(page_len);
scanned_count = scanned_count.saturating_add(page_len);
```

```rust
// crates/aether-dispatch-core/src/sequence.rs:31
.map(|(index, candidate)| DispatchSequenceItem {
    candidate_index: u32::try_from(index).unwrap_or(u32::MAX),
    retry_index: 0,
    candidate,
    mark: DispatchSequenceMark::Pending,
})
```

Rules:

- Do not use unchecked casts for indexes or scanned counts.
- Do not use `unwrap()` on caller-controlled numeric conversions.
- Keep `max_scan`, `page_size`, and `window_size` normalized before cursor loops.

## Test Patterns

### Pool cursor tests

Pool cursor tests use `#[tokio::test]`, a small in-memory `TestPort`, and assertions on both outcome and observed read limits.

```rust
// crates/aether-dispatch-core/src/pool.rs:166
#[tokio::test]
async fn small_pool_is_returned_in_one_frozen_window() {
    let mut port = TestPort {
        pages: VecDeque::from([vec![3, 1, 2]]),
        read_limits: Vec::new(),
    };

    let outcome = run_pool_dispatch_cursor(&mut port, PoolWindowConfig::default())
        .await
        .unwrap();

    assert_eq!(outcome.candidates, [1, 2, 3]);
    assert_eq!(outcome.scanned_count, 3);
    assert_eq!(port.read_limits, [64]);
}
```

```rust
// crates/aether-dispatch-core/src/pool.rs:199
#[tokio::test]
async fn max_scan_caps_page_reads() {
    let mut port = TestPort {
        pages: VecDeque::from([Vec::new()]),
        read_limits: Vec::new(),
    };
    let config = PoolWindowConfig {
        window_size: 16,
        page_size: 64,
        max_scan: 32,
    };

    let outcome = run_pool_dispatch_cursor(&mut port, config).await.unwrap();

    assert!(outcome.exhausted);
    assert_eq!(port.read_limits, [32]);
}
```

### Sequence tests

Sequence tests should verify cursor movement, marks, and ordering.

```rust
// crates/aether-dispatch-core/src/sequence.rs:96
#[test]
fn mark_failed_advances_without_reordering() {
    let mut sequence = DispatchSequence::from_candidates(vec!["a", "b", "c"]);

    assert_eq!(sequence.next().map(|item| item.candidate), Some("a"));
    assert_eq!(sequence.mark_failed().map(|item| item.candidate), Some("b"));
    assert_eq!(sequence.next().map(|item| item.candidate), Some("b"));
    assert_eq!(sequence.mark_failed().map(|item| item.candidate), Some("c"));
    assert_eq!(sequence.next().map(|item| item.candidate), Some("c"));

    assert_eq!(sequence.items()[0].mark, DispatchSequenceMark::Failed);
    assert_eq!(sequence.items()[1].mark, DispatchSequenceMark::Failed);
    assert_eq!(sequence.items()[2].mark, DispatchSequenceMark::Pending);
}
```

### Downstream guard tests

Gateway architecture tests guard public API expectations:

```rust
// apps/aether-gateway/src/tests/architecture/ai_serving.rs:1340
let dispatch_core = read_workspace_file("crates/aether-dispatch-core/src/lib.rs");
for pattern in [
    "DispatchCandidateRef",
    "DispatchSequence",
    "PoolDispatchPort",
    "PoolWindowConfig",
    "DispatchEffect",
] {
    assert!(
        dispatch_core.contains(pattern),
        "aether-dispatch-core should export pure dispatch primitive {pattern}"
    );
}
```

Gateway also guards production default use in its pool cursor test:

```rust
// apps/aether-gateway/src/dispatch/pool_scheduler.rs:2269
let mut cursor = PoolKeyCursor::new(PlannerAppState::new(&app), group, None, None, None);
assert_eq!(
    cursor.window_size,
    aether_dispatch_core::DEFAULT_POOL_WINDOW_SIZE
);
assert_eq!(
    cursor.page_size,
    aether_dispatch_core::DEFAULT_POOL_PAGE_SIZE
);
assert_eq!(
    cursor.max_scanned_keys,
    aether_dispatch_core::DEFAULT_POOL_MAX_SCAN
);
```

## Anti-Patterns

- Do not add I/O, database queries, runtime state reads, cache access, task spawning, or logging to this crate.
- Do not move `PoolKeyCursor` production behavior from gateway into core without a separate design; current core cursor is a reusable contract and test-backed algorithm, while gateway owns runtime scheduling.
- Do not change serde field naming or enum tagging for existing public types.
- Do not add `Default` to types without a meaningful semantic empty value.
- Do not derive `Copy` for structs that own `String`, `Vec`, or generic candidates.
- Do not add broad trait bounds like `Debug`, `Clone`, or `Sync` to `PoolDispatchPort` associated types unless a real caller requires them.
- Do not use `impl Trait` in public data constructors when explicit generics preserve clearer API contracts.
- Do not introduce new dependencies directly in this crate without adding them through the workspace.
