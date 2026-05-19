# Kiro.rs Comparison

## Cache Tracker

`kiro.rs` has a dedicated Anthropic cache tracker that builds prefix
checkpoints from tools, system blocks, and message blocks. The important TTL
behavior is that cache hits do not refresh `expires_at`; duplicate updates also
leave the original expiry intact.

Aether now applies that TTL rule to Kiro web-search synthetic responses and
common Kiro stream usage through
`apps/aether-gateway/src/execution_runtime/kiro_cache.rs`:

- cache profile is built from the original Claude request when cacheable
  `cache_control` breakpoints exist;
- `cache_control` metadata is stripped before fingerprinting;
- tools, system blocks, and message blocks are flattened into prefix
  fingerprints;
- cache state is scoped by provider, endpoint, and key;
- prefix hits return `cache_read_input_tokens`;
- first writes and expired entries return `cache_creation_input_tokens`;
- hits do not extend expiry.

Remaining difference: Aether still uses a lightweight local token estimate,
where `kiro.rs` uses its tokenizer helpers. Remote `count_tokens` integration
was intentionally not added to Aether.

The Aether feature is gated by Kiro provider config:

```json
{
  "kiro": {
    "simulated_cache_enabled": true
  }
}
```

Missing config defaults to disabled, so existing Kiro providers do not emit
simulated cache read/write usage until an administrator enables the mode.

## Thinking

Aether already supports Kiro thinking conversion and stream emission. The
provider transport converter generates the thinking prefix and the ai-formats
Kiro stream state emits `thinking_delta` blocks.

`kiro.rs` also has extra compression logic for thinking blocks. No failing case
was found in Aether during this task, and the ai-formats Kiro tests passed.

## Tool Use

Aether already preserves `tool_use` and `tool_result` in Kiro conversion and
stream handling. The provider transport Kiro tests passed.

`kiro.rs` has additional repair/compression behavior around tool-use pairs. No
current Aether failure was found in this pass; treat that as a follow-up only if
a concrete pairing regression appears.

## Web Search

Aether has a gateway-level Kiro MCP shortcut for `web_search`. Before this task
the synthetic SSE hardcoded cache usage to zero. It now injects computed cache
usage in both `message_start` and `message_delta`, and reports billed input
tokens by subtracting cache creation/read usage. That matches the shape used by
`kiro.rs` web-search events.

The query extraction was also aligned with `kiro.rs`: Aether now uses the last
user message from the original Claude request instead of the first message,
which is important for multi-turn web-search flows.

The routing strategy differs: `kiro.rs` also strips mixed web-search tools
before forwarding non-local requests. Aether's current shortcut only handles the
pure built-in web-search case, and this task leaves that behavior unchanged.
