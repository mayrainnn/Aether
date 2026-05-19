# Kiro Cache Optimization Design

## Scope

This task implements the fourth point first: Kiro cache read/write accounting
and TTL behavior. The first three points are treated as compatibility checks:
thinking, tool use, and web search should remain unchanged unless comparison
with `/Volumes/mayrain/workspace/private/kiro.rs` exposes a concrete defect.

## Current Aether Shape

Kiro request conversion is centered in
`crates/aether-provider-transport/src/kiro/converter.rs` and
`crates/aether-provider-transport/src/kiro/request.rs`.

Kiro stream compatibility is centered in
`crates/aether-ai-formats/src/provider_compat/kiro_stream.rs` and the
`provider_compat/kiro_stream/state/` modules.

Kiro web search shortcut handling is centered in
`apps/aether-gateway/src/execution_runtime/kiro_web_search.rs`.

Canonical usage conversion already has cache fields in
`crates/aether-ai-formats/src/protocol/canonical.rs`, so the schema support is
present. The missing piece is runtime accounting for Kiro-specific cache
profiles and first-write TTL semantics.

## Reference Behavior From `kiro.rs`

The reference implementation has a dedicated
`src/anthropic/cache_tracker.rs`:

- Build a cache profile from request content and cache-control breakpoints.
- Strip `cache_control` when hashing cacheable content.
- Track cache entries by credential and profile prefix.
- On a hit, compute `cache_read_input_tokens`.
- On a miss or partial hit, compute `cache_creation_input_tokens`.
- Do not refresh `expires_at` on a hit.
- Do not refresh `expires_at` during update when the same breakpoint already
  exists.

This is the key TTL correction: Anthropic-style cache TTL is measured from the
first write, not from the latest read.

## Fourth Point Design

Add a Kiro cache-accounting path that is local to the Kiro runtime boundary and
does not change the general provider request builder unless required.

Target responsibilities:

- Build a deterministic Kiro cache profile from the incoming Claude-shaped
  request body.
- Preserve `cache_control` semantics for cache breakpoints, including default
  ephemeral 5m TTL and explicit 1h TTL.
- Normalize profile content before hashing so `cache_control` metadata does not
  make otherwise-identical content miss.
- Store cache entries under a credential/provider identity so users do not
  share cache accounting state.
- Compute usage fields compatible with Claude-style response usage:
  `cache_read_input_tokens` and `cache_creation_input_tokens`.
- Keep expiry anchored to first insertion. Reads and duplicate updates must not
  extend `expires_at`.

Integration covers both the Kiro synthetic web-search path and the common Kiro
stream path. The tracker itself lives in
`apps/aether-gateway/src/execution_runtime/kiro_cache.rs`, not in the
web-search renderer, so web-search and stream finalization share the same
profile/tracker boundary. Runtime mutable state stays out of
`aether-ai-formats`.

The cache profile now follows the same broad shape as `kiro.rs`: flatten tools,
system blocks, and message blocks; strip `cache_control`; build canonical prefix
fingerprints; and resolve the newest matching cacheable breakpoint. Synthetic
web-search SSE uses billed input tokens by subtracting cache creation/read
tokens from the estimated input tokens.

Simulated cache accounting is disabled by default. Kiro providers opt in through
`provider.config.kiro.simulated_cache_enabled`. Missing config and non-Kiro
providers do not emit simulated cache read/write usage.

## First Three Points Check

### Thinking

Aether already has thinking support in the Kiro converter and stream state.
The converter creates a thinking prefix, and the Kiro stream state emits
thinking deltas. The reference repo has similar prefix behavior plus extra
compression utilities. No change is planned unless tests reveal that cache
normalization drops thinking blocks.

### Tool Use

Aether already handles `tool_use` and `tool_result` in Kiro conversion and
streaming. The reference repo also repairs tool-use pairs and has compressor
logic. No change is planned unless the comparison finds a broken pairing case
in Aether's current converter.

### Web Search

Aether has a gateway-level Kiro MCP shortcut for `web_search`. This is
functionally similar to the reference repo's web-search bridge. Cache accounting
is added here first, including billed input token reporting and last-user query
extraction for multi-turn conversations. Mixed web-search tool stripping remains
outside this task unless a concrete Aether failure appears.

## Impact Notes

GitNexus impact for `build_kiro_provider_request_body` returned CRITICAL risk
with 26 impacted symbols, 7 direct callers, 3 affected processes, and 7 modules.
Avoid changing this symbol unless the cache work cannot be localized.

The lower-risk direction is to add a focused helper for Kiro cache accounting
and wire it into gateway runtime usage reporting. If provider-transport must be
changed, re-run GitNexus impact on the exact symbols and warn before editing.

## Data Flow

Incoming Kiro Claude request body
-> cache profile builder
-> cache tracker compute/update
-> Claude-style usage fields
-> synthetic Kiro SSE usage / canonical usage bridge
-> client-visible cache read/write numbers.

## Compatibility

- Do not change request payload shape sent to upstream providers unless needed.
- Do not change thinking/tool/web-search behavior except for cache usage fields.
- Keep cache accounting best-effort and local; failures should not block the
  actual Kiro web-search response.
- Do not put runtime mutable state in `aether-ai-formats`; keep that crate pure.
- Do not add remote `count_tokens` integration; token accounting remains local
  and Kiro-scoped.
