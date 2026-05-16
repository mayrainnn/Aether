# Implementation Plan

## Spec Context

Read before editing:

* `.trellis/spec/aether-contracts/backend/protocol-contracts.md`
* `.trellis/spec/aether-provider-transport/backend/streaming-and-request-building.md`
* `.trellis/spec/aether-gateway/backend/request-execution-guidelines.md`
* `.trellis/spec/aether-gateway/backend/quality-guidelines.md`

## Checklist

1. Inspect current diff and isolate Grok browser profile write/read points:
   * `crates/aether-provider-transport/src/network.rs`
   * `crates/aether-provider-transport/src/grok.rs`
   * `apps/aether-gateway/src/execution_runtime/transport.rs`
   * `apps/aether-gateway/src/execution_runtime/grok.rs`
   * `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/grok.rs`
   * Grok import/update payload handlers
2. Add a provider-transport profile helper for Grok/browser profile metadata.
3. Change Grok import/update path so new inferred browser profile is written to
   `key.fingerprint.transport_profile` when no explicit fingerprint is supplied.
4. Keep `resolve_transport_profile` fallback order:
   key fingerprint -> provider config -> legacy Grok auth config -> Grok default.
5. Refactor Grok header builders to derive UA and Client Hints from the resolved
   browser profile helper, avoiding quota/runtime constant drift.
6. Ensure `browser_wreq` execution remains generic and branches only on
   `ResolvedTransportProfile.backend`.
7. Update diagnostics or tests to prove fingerprint configuration is visible.
8. Update Trellis specs if implementation reveals a durable convention.

## Validation Commands

Use `rtk` prefix for this workspace:

```bash
rtk cargo test -p aether-provider-transport grok --lib
rtk cargo test -p aether-provider-transport network --lib
rtk cargo test -p aether-gateway direct_sync_execution_runtime_routes_browser_wreq_transport_in_process --lib
rtk cargo test -p aether-gateway quota::grok::tests --lib
rtk cargo check -p aether-gateway
```

Frontend validation only if import UI or payload types are changed:

```bash
rtk npm run test:run -- src/features/providers/components/__tests__/OAuthAccountDialog.grok-import.spec.ts
rtk npm run type-check
```

## Risk Points

* Do not change the serialized shape of existing `ResolvedTransportProfile`
  fields.
* Do not remove auth-config fallback until a migration exists.
* Do not make generic transport execution inspect provider names.
* Do not let unsupported profile values silently fall back to `chrome136`.
* Be careful with existing staged/user changes; this branch already contains a
  large Grok diff.

## Review Gate

Before implementation starts, confirm this decision:

New Grok account imports should write inferred browser transport into
`key.fingerprint.transport_profile`, with auth-config browser fields preserved
only as legacy fallback.
