# Fill aether-dispatch-core Backend Spec

## Goal
Analyze `crates/aether-dispatch-core` and fill all spec files under `.trellis/spec/aether-dispatch-core/backend/` with real code examples, patterns, and anti-patterns.

## Context
`aether-dispatch-core` is a pure domain-type crate for request-scoped dispatch. It provides:
- **Dispatch candidates** (`DispatchCandidateRef`, `DispatchRankFacts`, `KeyRef`, `PoolRef`, `ProviderEndpointRef`) — lightweight reference types that identify a dispatchable upstream
- **Dispatch effects** (`DispatchEffect`, `DispatchEffectKind`) — enum representing side-effects produced by dispatch decisions
- **Pool dispatch cursor** (`run_pool_dispatch_cursor`, `PoolDispatchCursorOutcome`, `PoolDispatchPort`, `PoolDispatchWindow`) — cursor-based pagination over pool members, with configurable window/page sizes
- **Dispatch sequence** (`DispatchSequence`, `DispatchSequenceItem`, `DispatchSequenceMark`) — ordered sequence of dispatch attempts with position marking

### Dependency Graph
- **Internal deps**: None (no other Aether crate dependency — leaf crate)
- **External deps**: `async-trait`, `serde`, `serde_json`, `thiserror`
- **Consumers**: `apps/aether-gateway` uses these types in its dispatch/pool_scheduler modules

### File Layout
```
crates/aether-dispatch-core/src/
├── lib.rs          (15 lines — re-exports)
├── candidate.rs    (68 lines — DispatchCandidateRef, KeyRef, PoolRef, etc.)
├── effects.rs      (16 lines — DispatchEffect enum)
├── pool.rs         (216 lines — cursor, window, port types + cursor logic)
└── sequence.rs     (110 lines — DispatchSequence, items, marks)
```

Total: ~850 lines. This is a small, pure-data crate with no I/O, no database, no logging.

## Requirements

### Files to Fill

#### `.trellis/spec/aether-dispatch-core/backend/index.md`
- Package summary with Cargo.toml evidence
- Public API surface (re-exports from lib.rs)
- Guidelines index table linking to other spec files
- Pre-development checklist
- Known consumers (gateway dispatch modules)
- Quality gate commands

#### `.trellis/spec/aether-dispatch-core/backend/directory-structure.md`
- Crate layout diagram
- Module ownership (what each file owns)
- Expansion rules (when to add new files)

#### `.trellis/spec/aether-dispatch-core/backend/error-handling.md`
- `PoolDispatchError` enum and its variants
- `thiserror` derive patterns
- Where errors bubble up to (gateway dispatch layer)

#### `.trellis/spec/aether-dispatch-core/backend/quality-guidelines.md`
- Serialization contracts (serde derives, JSON shape)
- `async-trait` usage in `PoolDispatchPort`
- Visibility rules (what's pub vs pub(crate))
- Test patterns from existing tests
- Anti-patterns to avoid

### Rules

1. **Spec files are NOT fixed — adapt to reality**
   - Delete template files that don't apply (e.g., no `database-guidelines.md` since this crate has no DB)
   - Create new files for patterns templates don't cover
   - Rename files if template names don't fit
   - Update index.md to reflect the final set

2. **Parallel agents — stay in your lane**
   - ONLY modify files under `.trellis/spec/aether-dispatch-core/`
   - DO NOT modify source code, other spec directories, or task files
   - DO NOT run git commands
   - You may read any file for analysis

## Acceptance Criteria

- [ ] Real code examples from the actual codebase (with file paths like `// crates/aether-dispatch-core/src/pool.rs:73`)
- [ ] Anti-patterns documented
- [ ] No placeholder text remaining
- [ ] index.md reflects actual file set
- [ ] No `database-guidelines.md` or `logging-guidelines.md` (this crate has no DB or logging)

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
| `get_repo_structure` | Full file listing | `get_repo_structure({repo_name: "aether-dispatch-core"})` |
| `get_file_structure` | All nodes in a file | `get_file_structure({repo_name: "aether-dispatch-core", file_path: "src/pool.rs"})` |
| `get_ast_node` | Code + deps + refs | `get_ast_node({repo_name: "aether-dispatch-core", node_ids: [...]})` |

## Notes

- Package path: `crates/aether-dispatch-core/`
- Language: Rust, no framework (pure domain types)
- Build: `cargo test -p aether-dispatch-core`
- Lightweight task: PRD-only is sufficient for this small crate
