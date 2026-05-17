# Fill aether-pool-core Backend Spec

## Goal
Analyze `crates/aether-pool-core` and fill all spec files under `.trellis/spec/aether-pool-core/backend/` with real code examples, patterns, and anti-patterns.

## Context
`aether-pool-core` is the provider-independent pool scheduling and scoring engine. It is pure computation with no I/O, no async, no database, and no logging.

### Scheduling (`scheduler.rs`, ~1301 lines)
- `run_pool_scheduler` — top-level entry that groups candidates, applies presets, and produces `PoolSchedulerOutcome`
- `PoolCandidateInput` — input facts per candidate (key, provider, pool membership, cooldown, cost, priority, scores)
- `PoolScheduledCandidate` / `PoolSkippedCandidate` — scheduled vs skipped results
- `PoolSchedulingConfig` / `PoolSchedulingPreset` — configuration for scheduling behavior
- `PoolRuntimeState` — per-candidate runtime signals (cooldown, cost exhaustion, sticky affinity)
- Distribution modes: load-balance, single-account, priority
- Presets: plan-preset with catalog context, priority strategy
- Grouping: interleaved candidates across provider groups, internal key reordering

### Scoring (`scoring.rs`, ~540 lines)
- `score_pool_member` / `score_pool_member_with_rules` — main scoring functions
- `probe_freshness_score` / `probe_freshness_score_with_ttl` — probe-based freshness scoring
- `PoolMemberScoreInput` / `PoolMemberScoreOutput` — scoring I/O types
- `PoolMemberScoreWeights` — configurable weight factors
- `PoolMemberScoreRules` — rule overrides
- Constants: `POOL_SCORE_VERSION`, `PROBE_FAILURE_PENALTY`, `REQUEST_FAILURE_PENALTY`, etc.

### Dependency Graph
- **Internal deps**: `aether-data-contracts` (for stored type contracts)
- **External deps**: `serde_json`
- **Consumers**: `apps/aether-gateway` (`apply_local_execution_pool_scheduler_with_runtime_map`), `crates/aether-provider-pool`

### Key Algorithmic Patterns
- Pool candidates are grouped by provider+account, then interleaved
- Sticky-hit promotion happens before other sorted keys
- Load-balance distribution ignores sticky hits
- Single-account distribution orders by priority then reverse-LRU
- Cooldown and cost-exhausted keys are skipped before scheduling
- Plan presets combine with catalog context for tiered scheduling

### File Layout
```
crates/aether-pool-core/src/
├── lib.rs          (17 lines — re-exports)
├── scheduler.rs    (1301 lines — pool scheduling algorithm, tests at line 808+)
└── scoring.rs      (540 lines — member scoring)
```

Total: ~3,716 lines including ~500 lines of tests.

## Requirements

### Files to Fill

#### `.trellis/spec/aether-pool-core/backend/index.md`
- Package summary with Cargo.toml evidence
- Public API surface (scheduler + scoring re-exports)
- Guidelines index table
- Pre-development checklist
- Known consumers
- Quality gate (`cargo test -p aether-pool-core`)

#### `.trellis/spec/aether-pool-core/backend/directory-structure.md`
- Crate layout (only 3 source files but dense)
- Module ownership boundaries
- Expansion rules

#### `.trellis/spec/aether-pool-core/backend/scheduler-domain-patterns.md` (crate-specific)
- Scheduling algorithm flow: group → annotate → skip → sort → schedule
- Distribution modes (load-balance, single-account, priority)
- Preset system (PoolSchedulingPreset, plan-presets)
- Sticky affinity mechanism
- Input/output type contracts
- Test patterns for scheduling (test functions at scheduler.rs:808+)

#### `.trellis/spec/aether-pool-core/backend/error-handling.md`
- No Result types in main scheduling — all outputs are `PoolSchedulerOutcome`
- How errors manifest as `PoolSkippedCandidate` entries
- Skip reasons: `POOL_ACCOUNT_BLOCKED_SKIP_REASON`, `POOL_COOLDOWN_SKIP_REASON`, etc.

#### `.trellis/spec/aether-pool-core/backend/quality-guidelines.md`
- Pure-function discipline (no I/O, no side effects)
- Scoring weight constants and versioning
- Test coverage expectations (500+ lines of scheduler tests)
- Anti-patterns: don't add async, don't add logging, don't add DB queries

### Rules

1. **Spec files are NOT fixed — adapt to reality**
   - Delete template files that don't apply (no `database-guidelines.md`, no `logging-guidelines.md`)
   - Create new files for patterns templates don't cover (e.g., `scheduler-domain-patterns.md`)
   - Update index.md to reflect the final set

2. **Parallel agents — stay in your lane**
   - ONLY modify files under `.trellis/spec/aether-pool-core/`
   - DO NOT modify source code, other spec directories, or task files
   - DO NOT run git commands
   - You may read any file for analysis

## Acceptance Criteria

- [ ] Real code examples from the actual codebase (with file paths)
- [ ] Scheduler algorithm flow documented with input/output contracts
- [ ] Scoring weights and constants documented
- [ ] Anti-patterns documented (no async, no I/O, no logging)
- [ ] No placeholder text remaining
- [ ] index.md reflects actual file set

## Tools Available

You have two MCP servers configured — use both for accurate specs:

### GitNexus MCP (architecture-level: clusters, execution flows, impact)
| Tool | Purpose | Example |
|------|---------|---------|
| `gitnexus_query` | Find execution flows by concept | `gitnexus_query({query: "...", repo: "Aether"})` |
| `gitnexus_context` | 360-degree symbol view | `gitnexus_context({name: "ClassName", repo: "Aether"})` |
| `gitnexus_impact` | Blast radius analysis | `gitnexus_impact({target: "X", direction: "upstream", repo: "Aether"})` |
| `gitnexus_cypher` | Direct graph queries | `gitnexus_cypher({query: "MATCH ...", repo: "Aether"})` |

### ABCoder MCP (symbol-level: AST nodes, signatures, cross-file deps)
| Tool | Purpose | Example |
|------|---------|---------|
| `get_repo_structure` | Full file listing | `get_repo_structure({repo_name: "aether-pool-core"})` |
| `get_file_structure` | All nodes in a file | `get_file_structure({repo_name: "aether-pool-core", file_path: "src/scheduler.rs"})` |
| `get_ast_node` | Code + deps + refs | `get_ast_node({repo_name: "aether-pool-core", node_ids: [...]})` |

## Notes

- Package path: `crates/aether-pool-core/`
- Language: Rust, no framework (pure computation)
- Build: `cargo test -p aether-pool-core`
- Key insight: scheduler.rs has ~500 lines of tests (808-1300) — use them to document expected behavior
- Lightweight task: PRD-only is sufficient
