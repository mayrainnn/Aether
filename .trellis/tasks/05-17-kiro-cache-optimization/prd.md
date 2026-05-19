# kiro cache optimization

## Goal

Optimize Kiro cache read/write behavior in Aether so repeated Kiro requests report cache usage and TTL behavior that matches upstream prompt-cache semantics.

## Requirements

- Aether must preserve the existing Kiro thinking/tool/web search support.
- Kiro cache usage must be accounted in a way that matches upstream hit behavior across repeated requests.
- Cache TTL must be treated as first-write based, not extended on cache hits.
- Cache read/write token fields must remain compatible with the canonical usage schema already used by `aether-ai-formats`.
- Add focused regression tests for first write, repeated hit, and TTL-expiry behavior.
- Simulated cache accounting must be Kiro-only and disabled by default behind provider configuration.

## Confirmed Facts

- `aether-provider-transport` already converts Kiro Claude-style requests, including thinking prefixes and tool results, in `crates/aether-provider-transport/src/kiro/converter.rs`.
- `aether-ai-formats` already preserves Kiro thinking, tool use, tool result, and cache usage fields in canonical protocol conversion.
- `apps/aether-gateway/src/execution_runtime/kiro_web_search.rs` already synthesizes Kiro `web_search` output through MCP.
- The private reference implementation at `/Volumes/mayrain/workspace/private/kiro.rs` uses a dedicated `CacheTracker` with TTL-aware cache profile computation and explicitly does not refresh `expires_at` on cache hits.
- Aether does not currently have a Kiro-specific `CacheTracker` equivalent in the inspected source.

## Acceptance Criteria

- [x] Repeated Kiro requests report stable cache read/write usage after the first hit.
- [x] Cache expiry is measured from the original write, not extended by hits.
- [x] Thinking, tool use, and web search behavior remain unchanged.
- [x] Regression tests cover first write, repeat hit, and expiry behavior.
- [x] Provider config can enable Kiro simulated cache accounting, with missing config defaulting to disabled.
- [x] Admin UI exposes the Kiro-only `模拟缓存模式` toggle without changing non-Kiro providers.

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
