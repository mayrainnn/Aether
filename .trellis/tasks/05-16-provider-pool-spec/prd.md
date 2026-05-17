# Fill aether-provider-pool Backend Spec

## Goal
Analyze `crates/aether-provider-pool` and fill all spec files under `.trellis/spec/aether-provider-pool/backend/` with real code examples, patterns, and anti-patterns.

## Context
`aether-provider-pool` is the provider-specific pool behavior adapter layer. It bridges `aether-pool-core` (generic scheduling/scoring) with concrete provider implementations.

### Core Abstractions
- **`ProviderPoolAdapter`** (trait) — adapter interface for provider-specific pool behavior: quota checking, plan tier derivation, capability reporting
- **`ProviderPoolService`** — registry of adapters, looked up by provider type string
- **`ProviderPoolCapabilities` / `ProviderPoolCapability`** — bitflag-style capability set (PlanTier, QuotaRefresh, etc.)

### Provider Implementations (`providers/`)
Seven provider adapters:
1. **`CodexProviderPoolAdapter`** — Codex/claude_code provider: quota via WHAM usage URL
2. **`KiroProviderPoolAdapter`** — Kiro provider: usage limits endpoint
3. **`ChatGptWebProviderPoolAdapter`** — ChatGPT web: conversation init + image quota
4. **`AntigravityProviderPoolAdapter`** — Antigravity: fetch available models
5. **`VertexAiProviderPoolAdapter`** — Vertex AI (via default/unsupported adapter)
6. **`GeminiCliProviderPoolAdapter`** — Gemini CLI (via default/unsupported adapter)
7. **`DefaultProviderPoolAdapter`** / **`UnsupportedQuotaProviderPoolAdapter`** — fallback

### Plan & Preset Logic
- `derive_plan_tier` / `derive_oauth_plan_type` — map provider metadata to plan tiers
- `normalize_provider_scheduling_presets` — normalize admin-provided scheduling presets
- `build_admin_pool_scheduling_presets_payload` — build admin API payload

### Quota Management
- `provider_pool_key_account_quota_exhausted` — check if account quota is exhausted
- `provider_pool_member_quota_snapshot` — snapshot current quota state
- `ProviderPoolQuotaRequestSpec` — spec for quota refresh requests

### Dependency Graph
- **Internal deps**: `aether-data-contracts` (for `StoredProviderCatalogKey`), `aether-pool-core` (for `PoolSchedulingPreset`)
- **External deps**: `serde_json`, `url`, `uuid`
- **Consumers**: `apps/aether-gateway` dispatch/pool modules, admin API handlers

### File Layout
```
crates/aether-provider-pool/src/
├── lib.rs           (373 lines — re-exports + test module)
├── capability.rs    (23 lines — capability bitflags)
├── plan.rs          (104 lines — plan tier derivation)
├── presets.rs       (244 lines — scheduling preset normalization)
├── provider.rs      (107 lines — ProviderPoolAdapter trait)
├── quota.rs         (263 lines — quota snapshot/exhaustion helpers)
├── quota_refresh.rs (19 lines — quota refresh request spec)
├── service.rs       (132 lines — ProviderPoolService registry)
└── providers/
    ├── mod.rs           (29 lines)
    ├── antigravity.rs   (77 lines)
    ├── chatgpt_web.rs   (272 lines)
    ├── codex.rs         (136 lines)
    ├── default.rs       (10 lines)
    ├── kiro.rs          (167 lines)
    └── unsupported.rs   (47 lines)
```

Total: ~3,268 lines.

## Requirements

### Files to Fill

#### `.trellis/spec/aether-provider-pool/backend/index.md`
- Package summary with Cargo.toml evidence
- Public API surface (re-exports from lib.rs)
- Adapter pattern overview (trait + registry + per-provider impls)
- Guidelines index table
- Pre-development checklist
- Known consumers
- Quality gate (`cargo test -p aether-provider-pool`)

#### `.trellis/spec/aether-provider-pool/backend/directory-structure.md`
- Crate layout with providers/ subdirectory
- Module ownership
- Provider adapter pattern: trait in `provider.rs`, impls in `providers/*.rs`
- Expansion rules (how to add a new provider adapter)

#### `.trellis/spec/aether-provider-pool/backend/error-handling.md`
- No Result-based error propagation in adapter trait
- How adapter failures surface to callers
- Validation in `plan.rs` and `presets.rs`

#### `.trellis/spec/aether-provider-pool/backend/quality-guidelines.md`
- Adapter pattern discipline (trait compliance, provider isolation)
- Capability bitflag usage
- Test patterns: each adapter has unit tests in lib.rs
- Anti-patterns: don't add HTTP clients, don't add DB queries
- When to add a new provider adapter vs extend existing one

### Rules

1. **Spec files are NOT fixed — adapt to reality**
   - Delete template files that don't apply (no `database-guidelines.md`, no `logging-guidelines.md`)
   - Create new files for patterns templates don't cover
   - Update index.md to reflect the final set

2. **Parallel agents — stay in your lane**
   - ONLY modify files under `.trellis/spec/aether-provider-pool/`
   - DO NOT modify source code, other spec directories, or task files
   - DO NOT run git commands
   - You may read any file for analysis

## Acceptance Criteria

- [ ] Real code examples from the actual codebase (with file paths)
- [ ] Adapter pattern documented with trait contract and all 7 provider impls
- [ ] Provider-specific quota/plan logic documented per provider
- [ ] Anti-patterns documented
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
| `get_repo_structure` | Full file listing | `get_repo_structure({repo_name: "aether-provider-pool"})` |
| `get_file_structure` | All nodes in a file | `get_file_structure({repo_name: "aether-provider-pool", file_path: "src/providers/codex.rs"})` |
| `get_ast_node` | Code + deps + refs | `get_ast_node({repo_name: "aether-provider-pool", node_ids: [...]})` |

## Notes

- Package path: `crates/aether-provider-pool/`
- Language: Rust, no framework (pure domain logic)
- Build: `cargo test -p aether-provider-pool`
- Key insight: `ProviderPoolService` acts as a registry with `with_builtin_adapters()` constructor
- Lightweight task: PRD-only is sufficient
