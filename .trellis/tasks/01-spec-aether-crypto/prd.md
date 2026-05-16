# Fill aether-crypto backend spec

## Goal

Analyze the `aether-crypto` crate and fill all spec files in `.trellis/spec/aether-crypto/backend/` with real, project-specific coding guidelines derived from the actual source code.

## Context

**Project**: Aether — a multi-provider AI API gateway written in Rust (axum + SeaORM + tokio).

**This crate**: `crates/aether-crypto/`
- **Layer**: Foundation
- **Purpose**: Cryptographic utilities: AES encryption/decryption, key derivation, token signing
- **Internal dependencies**: (none — leaf crate)
- **Key patterns**: Pure utility crate, no async, no state

**Architecture overview** (for cross-referencing):
- Layer 0 (Foundation): crypto, wallet, cache, http, contracts, data-schema, ai-formats — zero internal deps
- Layer 1 (Core): data-contracts, oauth, runtime — minimal deps
- Layer 2 (Domain): runtime-state, scheduler-core, video-tasks-core, task-runtime, data, provider-transport
- Layer 3 (Services): ai-serving, model-fetch, usage-runtime, billing
- Layer 4 (Application): admin, gateway, testkit
- Layer 5 (Binary): proxy

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
| `get_repo_structure` | Full file listing | `get_repo_structure({repo_name: "aether-crypto"})` |
| `get_file_structure` | All nodes in a file | `get_file_structure({repo_name: "aether-crypto", file_path: "src/..."})` |
| `get_ast_node` | Code + deps + refs | `get_ast_node({repo_name: "aether-crypto", node_ids: [...]})` |

### Recommended Workflow
1. `get_repo_structure` to see all files in this crate
2. `get_file_structure` on key files to understand module layout
3. `get_ast_node` on important structs/traits/functions to get exact signatures
4. GitNexus `gitnexus_query` to find execution flows this crate participates in
5. Read source files directly for full context where needed
6. Write specs with real code examples from steps 2-5

## Files to Fill

All files are in `.trellis/spec/aether-crypto/backend/`:

1. **directory-structure.md** — Document the actual module layout, what each file/module does, and the organizational pattern used.

2. **error-handling.md** — Document error types defined in this crate, how errors propagate (Result types, ? operator chains), error conversion patterns (From/Into impls), and how errors surface to callers.

3. **quality-guidelines.md** — Document code standards: naming conventions, visibility patterns (pub/pub(crate)/private), type safety patterns, forbidden patterns (things the code avoids), and testing patterns used.

4. **logging-guidelines.md** — Document tracing/logging usage: which tracing macros are used, span patterns, structured fields, log levels for different events.

5. **database-guidelines.md** — If this crate interacts with databases (SeaORM, Redis, etc.), document query patterns, transaction usage, connection handling. If NOT applicable, delete this file and note it in index.md.

6. **index.md** — Update to reflect the final set of files (add/remove as needed).

## Important Rules

### Spec files are NOT fixed — adapt to reality
- Delete template files that don't apply (e.g., database-guidelines.md for a pure utility crate)
- Create new files for patterns templates don't cover (e.g., `streaming-patterns.md` for provider-transport)
- Rename files if template names don't fit
- Update index.md to reflect the final set

### Content requirements
- Every guideline MUST include at least one real code example with file path (e.g., `src/lib.rs:45`)
- Document anti-patterns with "DON'T" examples where relevant
- Be specific to THIS crate, not generic Rust advice
- Include the actual type names, function signatures, and module paths from the source

### Parallel agents — stay in your lane
- ONLY modify files under `.trellis/spec/aether-crypto/backend/`
- DO NOT modify source code, other spec directories, or task files
- DO NOT run git commands
- You may read any file for analysis

## Acceptance Criteria
- [ ] Real code examples from the actual codebase (with file paths)
- [ ] Anti-patterns documented where relevant
- [ ] No placeholder text remaining (no "To be filled")
- [ ] No HTML comments remaining
- [ ] index.md reflects actual file set
- [ ] Each file is 60+ lines of substantive content

## Technical Notes
- Language: Rust (edition 2021)
- Async runtime: tokio
- Web framework: axum 0.8
- ORM: sea-orm (where applicable)
- Serialization: serde + serde_json
- Error handling: thiserror + anyhow (varies by crate)
- Logging: tracing crate
