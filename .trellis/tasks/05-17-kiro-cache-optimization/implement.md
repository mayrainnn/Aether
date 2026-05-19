# Kiro Cache Optimization Implementation Plan

## Preconditions

- Task stays in planning until this file and `design.md` are complete.
- Before code edits, run `trellis-before-dev` context for touched packages.
- Before editing any symbol, run GitNexus impact on the exact function, class,
  method, or helper to be changed.
- If impact is HIGH or CRITICAL, report it before proceeding.

## Execution Checklist

1. Refresh code context
   - Read Kiro converter, Kiro request builder, Kiro stream builders, gateway
     Kiro web-search path, canonical usage conversion, and existing cache
     primitives.
   - Compare relevant reference files in `/Volumes/mayrain/workspace/private/kiro.rs`.

2. Add focused cache-accounting tests first
   - First write returns cache creation tokens.
   - Repeated identical request returns cache read tokens.
   - Cache hit does not extend expiry.
   - Expired entry returns creation tokens again.
   - Profile hashing ignores `cache_control` metadata.

3. Implement Kiro cache profile and tracker
   - Implement the reusable helper in
     `apps/aether-gateway/src/execution_runtime/kiro_cache.rs`.
   - Keep the helper deterministic and unit-testable without network or MCP.
   - Preserve first-write TTL semantics and prefix-hit accounting without
     coupling the tracker to the web-search renderer.

4. Wire usage into Kiro web-search SSE and common Kiro streams
   - Replace hardcoded zero cache usage in the Kiro web-search synthetic
     response with computed `cache_creation_input_tokens` and
     `cache_read_input_tokens`.
   - Seed common Kiro stream report context with estimated input tokens and
     optional simulated cache usage before Kiro stream rewriting emits usage.
   - Use billed input tokens in the SSE usage payload.
   - Extract the search query from the last user message so multi-turn prompts
     do not fall back to the first turn.
   - Preserve current MCP result handling and error passthrough.

5. Add Kiro-only provider toggle
   - Read `provider.config.kiro.simulated_cache_enabled`.
   - Default missing config to disabled.
   - Expose the Kiro-only `模拟缓存模式` toggle in the provider form without
     changing non-Kiro providers.

6. Check first three features against `kiro.rs`
   - Thinking: verify existing conversion and stream tests still pass; report any
     missing compression or prefix behavior as a follow-up unless it is a cache
     regression.
   - Tool use: verify tool use/result handling still passes; report any pair
     repair divergence separately unless it breaks cache accounting.
   - Web search: verify synthetic web-search behavior still works, reports cache
     usage, and matches multi-turn query extraction.

7. Validation
   - Run focused Rust tests for the touched modules.
   - Run broader package tests when feasible:
     `cargo test -p aether-gateway`,
     `cargo test -p aether-ai-formats`,
     `cargo test -p aether-provider-transport` if touched.
   - Run `gitnexus_detect_changes({scope: "all", repo: "Aether"})` before any
     commit.

## Rollback Points

- If cache profile extraction becomes too broad, keep the change limited to
  web-search usage injection and document remaining upstream-parity gaps.
- If editing `build_kiro_provider_request_body` is required, stop and reassess
  because prior GitNexus impact was CRITICAL.
- If cache accounting cannot derive credible token counts from available request
  data, expose the limitation and avoid fake precision.

## Expected Deliverables

- Kiro cache tracker or equivalent helper with focused tests.
- Kiro web-search synthetic SSE usage reports non-zero cache read/write fields
  when applicable.
- Common Kiro streams report billed input and simulated cache read/write fields
  consistently when the provider toggle is enabled.
- Provider form exposes a Kiro-only simulated cache mode toggle, defaulting to
  disabled.
- A comparison note covering thinking, tool use, and web search against
  `kiro.rs`, including any remaining divergences.
