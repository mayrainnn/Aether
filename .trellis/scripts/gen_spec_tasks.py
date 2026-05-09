#!/usr/bin/env python3
"""Generate Codex spec-filling task PRDs for all Aether crates."""

import os
import json

WORKSPACE = "/Volumes/mayrain/workspace/Aether"
TASKS_DIR = f"{WORKSPACE}/.trellis/tasks"

# Architecture context per crate
CRATES = {
    # Layer 0 - Foundation (no internal deps)
    "aether-crypto": {
        "path": "crates/aether-crypto",
        "layer": "Foundation",
        "desc": "Cryptographic utilities: AES encryption/decryption, key derivation, token signing",
        "deps": [],
        "key_patterns": "Pure utility crate, no async, no state",
    },
    "aether-wallet": {
        "path": "crates/aether-wallet",
        "layer": "Foundation",
        "desc": "Wallet/balance abstractions for billing system",
        "deps": [],
        "key_patterns": "Value types, balance operations",
    },
    "aether-cache": {
        "path": "crates/aether-cache",
        "layer": "Foundation",
        "desc": "TTL-based in-memory cache with namespace support",
        "deps": [],
        "key_patterns": "Generic ExpiringMap<K,V>, CacheKeyNamespace enum",
    },
    "aether-http": {
        "path": "crates/aether-http",
        "layer": "Foundation",
        "desc": "HTTP utilities and shared request/response types",
        "deps": [],
        "key_patterns": "Thin wrapper types for axum",
    },
    "aether-contracts": {
        "path": "crates/aether-contracts",
        "layer": "Foundation",
        "desc": "Shared trait contracts and interfaces across the system",
        "deps": [],
        "key_patterns": "Trait definitions, no implementations",
    },
    "aether-data-schema": {
        "path": "crates/aether-data-schema",
        "layer": "Foundation",
        "desc": "Database schema definitions (SeaORM entities)",
        "deps": [],
        "key_patterns": "SeaORM Entity/Model/ActiveModel, migration definitions",
    },
    "aether-ai-formats": {
        "path": "crates/aether-ai-formats",
        "layer": "Foundation",
        "desc": "AI API format types: OpenAI, Claude, Gemini request/response structures",
        "deps": [],
        "key_patterns": "Serde structs for multi-provider AI API formats, conversion traits",
    },
    # Layer 1 - Core abstractions
    "aether-data-contracts": {
        "path": "crates/aether-data-contracts",
        "layer": "Core",
        "desc": "Data layer contracts: repository traits, query types, domain models",
        "deps": ["aether-ai-formats"],
        "key_patterns": "Repository trait pattern, domain model structs",
    },
    "aether-oauth": {
        "path": "crates/aether-oauth",
        "layer": "Core",
        "desc": "OAuth2 client implementations for provider authentication",
        "deps": ["aether-contracts"],
        "key_patterns": "OAuth flow types, token refresh logic",
    },
    "aether-runtime": {
        "path": "crates/aether-runtime",
        "layer": "Core",
        "desc": "Async runtime utilities: background task spawning, graceful shutdown",
        "deps": [],
        "key_patterns": "Tokio-based task management, JoinHandle tracking",
    },
    # Layer 2 - Domain logic
    "aether-runtime-state": {
        "path": "crates/aether-runtime-state",
        "layer": "Domain",
        "desc": "Runtime state management: provider health, circuit breakers, rate limits",
        "deps": ["aether-cache", "aether-data-contracts", "aether-runtime"],
        "key_patterns": "Shared mutable state behind Arc, health tracking",
    },
    "aether-scheduler-core": {
        "path": "crates/aether-scheduler-core",
        "layer": "Domain",
        "desc": "Provider scheduling: health checks, quota management, cooldown logic",
        "deps": ["aether-ai-formats", "aether-contracts", "aether-data-contracts", "aether-wallet"],
        "key_patterns": "Health/quota state machines, scheduling algorithms",
    },
    "aether-video-tasks-core": {
        "path": "crates/aether-video-tasks-core",
        "layer": "Domain",
        "desc": "Video generation task lifecycle: creation, polling, status tracking",
        "deps": ["aether-contracts", "aether-data-contracts"],
        "key_patterns": "Task state machine, async polling patterns",
    },
    "aether-task-runtime": {
        "path": "crates/aether-task-runtime",
        "layer": "Domain",
        "desc": "Generic background task runtime with persistence",
        "deps": ["aether-runtime"],
        "key_patterns": "Task trait, executor pattern",
    },
    "aether-data": {
        "path": "crates/aether-data",
        "layer": "Domain",
        "desc": "Data access layer: SeaORM repositories, Redis integration, query builders",
        "deps": ["aether-ai-formats", "aether-data-contracts", "aether-cache", "aether-wallet"],
        "key_patterns": "Repository implementations, connection pooling, transaction patterns",
    },
    "aether-provider-transport": {
        "path": "crates/aether-provider-transport",
        "layer": "Domain",
        "desc": "HTTP transport to AI providers: request building, streaming, error mapping",
        "deps": ["aether-ai-formats", "aether-contracts", "aether-crypto", "aether-data-contracts", "aether-oauth", "aether-runtime-state", "aether-video-tasks-core"],
        "key_patterns": "Provider adapters (OpenAI/Claude/Gemini/etc), streaming SSE, retry logic",
    },
    # Layer 3 - Services
    "aether-ai-serving": {
        "path": "crates/aether-ai-serving",
        "layer": "Services",
        "desc": "AI request serving logic: format adaptation, model mapping, response normalization",
        "deps": ["aether-ai-formats", "aether-contracts", "aether-scheduler-core"],
        "key_patterns": "Request/response adaptation pipeline, model alias resolution",
    },
    "aether-model-fetch": {
        "path": "crates/aether-model-fetch",
        "layer": "Services",
        "desc": "Model metadata fetching and caching from providers",
        "deps": ["aether-ai-formats", "aether-contracts", "aether-data-contracts", "aether-provider-transport", "aether-scheduler-core"],
        "key_patterns": "Periodic refresh, model list aggregation",
    },
    "aether-usage-runtime": {
        "path": "crates/aether-usage-runtime",
        "layer": "Services",
        "desc": "Usage tracking: token counting, cost calculation, queue-based recording",
        "deps": ["aether-ai-formats", "aether-contracts", "aether-data", "aether-data-contracts", "aether-runtime-state"],
        "key_patterns": "Event queue, async recording, cost formulas",
    },
    "aether-billing": {
        "path": "crates/aether-billing",
        "layer": "Services",
        "desc": "Billing logic: plan enforcement, quota checks, payment integration",
        "deps": ["aether-data-contracts", "aether-usage-runtime"],
        "key_patterns": "Plan/quota types, billing event processing",
    },
    # Layer 4 - Application
    "aether-admin": {
        "path": "crates/aether-admin",
        "layer": "Application",
        "desc": "Admin API handlers: provider management, system config, monitoring",
        "deps": ["aether-ai-formats", "aether-billing", "aether-contracts", "aether-data", "aether-data-contracts"],
        "key_patterns": "Axum handlers, admin-only middleware, CRUD operations",
    },
    "aether-gateway": {
        "path": "apps/aether-gateway",
        "layer": "Application",
        "desc": "Main API gateway: request routing, provider selection (planner), response finalization, streaming",
        "deps": ["aether-admin", "aether-ai-formats", "aether-ai-serving", "aether-billing", "aether-cache", "aether-contracts", "aether-crypto", "aether-data", "aether-data-contracts", "aether-http", "aether-model-fetch", "aether-oauth", "aether-provider-transport", "aether-scheduler-core", "aether-runtime"],
        "key_patterns": "Planner (candidate selection/ranking), finalize pipeline, SSE streaming, axum router composition",
    },
    "aether-testkit": {
        "path": "crates/aether-testkit",
        "layer": "Application",
        "desc": "Integration test utilities: mock providers, test fixtures, assertion helpers",
        "deps": ["aether-data", "aether-contracts", "aether-gateway", "aether-http", "aether-runtime", "aether-runtime-state"],
        "key_patterns": "Test harness, mock server builders, fixture factories",
    },
    # Layer 5 - Binary
    "aether-proxy": {
        "path": "apps/aether-proxy",
        "layer": "Binary",
        "desc": "Edge proxy binary: TLS termination, tunnel management, hardware detection, auto-registration",
        "deps": ["aether-contracts", "aether-http", "aether-runtime", "aether-runtime-state", "aether-gateway"],
        "key_patterns": "CLI app, TUI setup wizard, tunnel protocol, egress filtering",
    },
}

SPEC_FILES = [
    "directory-structure.md",
    "database-guidelines.md",
    "error-handling.md",
    "quality-guidelines.md",
    "logging-guidelines.md",
]

PRD_TEMPLATE = """# Fill {crate_name} backend spec

## Goal

Analyze the `{crate_name}` crate and fill all spec files in `.trellis/spec/{crate_name}/backend/` with real, project-specific coding guidelines derived from the actual source code.

## Context

**Project**: Aether — a multi-provider AI API gateway written in Rust (axum + SeaORM + tokio).

**This crate**: `{crate_path}/`
- **Layer**: {layer}
- **Purpose**: {desc}
- **Internal dependencies**: {deps_str}
- **Key patterns**: {key_patterns}

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
| `gitnexus_query` | Find execution flows by concept | `gitnexus_query({{query: "...", repo: "Aether"}})` |
| `gitnexus_context` | 360-degree symbol view | `gitnexus_context({{name: "ClassName", repo: "Aether"}})` |
| `gitnexus_impact` | Blast radius analysis | `gitnexus_impact({{target: "X", direction: "upstream", repo: "Aether"}})` |
| `gitnexus_cypher` | Direct graph queries | `gitnexus_cypher({{query: "MATCH ...", repo: "Aether"}})` |

### ABCoder MCP (symbol-level: AST nodes, signatures, cross-file deps)
| Tool | Purpose | Example |
|------|---------|---------|
| `get_repo_structure` | Full file listing | `get_repo_structure({{repo_name: "{crate_name}"}})` |
| `get_file_structure` | All nodes in a file | `get_file_structure({{repo_name: "{crate_name}", file_path: "src/..."}})` |
| `get_ast_node` | Code + deps + refs | `get_ast_node({{repo_name: "{crate_name}", node_ids: [...]}})` |

### Recommended Workflow
1. `get_repo_structure` to see all files in this crate
2. `get_file_structure` on key files to understand module layout
3. `get_ast_node` on important structs/traits/functions to get exact signatures
4. GitNexus `gitnexus_query` to find execution flows this crate participates in
5. Read source files directly for full context where needed
6. Write specs with real code examples from steps 2-5

## Files to Fill

All files are in `.trellis/spec/{crate_name}/backend/`:

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
- ONLY modify files under `.trellis/spec/{crate_name}/backend/`
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
"""

def generate_prd(crate_name, info):
    deps_str = ", ".join(info["deps"]) if info["deps"] else "(none — leaf crate)"
    return PRD_TEMPLATE.format(
        crate_name=crate_name,
        crate_path=info["path"],
        layer=info["layer"],
        desc=info["desc"],
        deps_str=deps_str,
        key_patterns=info["key_patterns"],
    )

def main():
    os.makedirs(TASKS_DIR, exist_ok=True)

    for i, (crate_name, info) in enumerate(CRATES.items(), start=1):
        slug = f"{i:02d}-spec-{crate_name}"
        task_dir = os.path.join(TASKS_DIR, slug)
        os.makedirs(task_dir, exist_ok=True)

        prd_path = os.path.join(task_dir, "prd.md")
        prd_content = generate_prd(crate_name, info)
        with open(prd_path, "w") as f:
            f.write(prd_content)

        print(f"[{i:02d}/24] {slug}")

    print(f"\nDone. {len(CRATES)} task PRDs created in {TASKS_DIR}/")

if __name__ == "__main__":
    main()
