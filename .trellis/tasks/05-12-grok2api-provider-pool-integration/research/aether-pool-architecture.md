# Aether Pool Architecture and Grok Integration Options

## Summary

Aether already has a pool-aware provider architecture with persisted provider/provider-endpoint/provider-key records, transport snapshot assembly, scheduler-side selectability rules, and dedicated admin pool routes. The Grok integration should reuse those layers and add a native Grok provider family, rather than introducing a separate Grok-only selection path or making `grok2api` the runtime control plane.

## Existing Aether Pool Architecture

### Persistent provider catalog

The provider catalog is the source of truth for providers, endpoints, and keys. Key records already carry auth, allowed models, upstream metadata, OAuth invalid markers, status snapshots, health buckets, and circuit breaker state.

Sources:
- [`crates/aether-data-contracts/src/repository/provider_catalog/types.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-data-contracts/src/repository/provider_catalog/types.rs#L140-L297)
- [`crates/aether-data-contracts/src/repository/provider_catalog/types.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-data-contracts/src/repository/provider_catalog/types.rs#L573-L694)
- [`crates/aether-data/src/repository/provider_catalog/postgres.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-data/src/repository/provider_catalog/postgres.rs#L15-L220)

### Transport snapshot and policy

`aether-provider-transport` reads the catalog, decrypts keys, absorbs safe auth config, and produces a runtime transport snapshot. Policy helpers then decide whether a transport is usable locally for OpenAI chat, same-format providers, Gemini, OAuth, proxy/profile, and provider-type support.

Sources:
- [`crates/aether-provider-transport/src/lib.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/lib.rs#L1-L115)
- [`crates/aether-provider-transport/src/snapshot.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/snapshot.rs#L14-L165)
- [`crates/aether-provider-transport/src/policy.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/policy.rs#L13-L220)
- [`crates/aether-provider-transport/src/provider_types.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/provider_types.rs#L107-L240)
- [`crates/aether-provider-transport/src/provider_types.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/provider_types.rs#L423-L524)

### Scheduler and candidate selection

The gateway scheduler combines provider quota snapshots, key RPM state, OAuth invalid markers, and pool state when selecting candidates. Pool-enabled providers are treated specially: account quota exhaustion and OAuth invalid flags are masked for pool-group candidates, and `provider.config.pool_advanced.skip_exhausted_accounts` controls whether quota exhaustion should exclude a key.

Sources:
- [`apps/aether-gateway/src/scheduler/candidate/runtime.rs`](/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/scheduler/candidate/runtime.rs#L22-L186)
- [`apps/aether-gateway/src/dispatch/pool_scheduler.rs`](/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/dispatch/pool_scheduler.rs#L813-L867)
- [`apps/aether-gateway/src/dispatch/pool_scheduler.rs`](/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/dispatch/pool_scheduler.rs#L870-L883)
- [`apps/aether-gateway/src/dispatch/pool_scheduler.rs`](/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/dispatch/pool_scheduler.rs#L971-L1018)

### Admin surface

Aether already has a dedicated admin pool route family with overview, scheduling presets, key listing, scores, selection resolution, batch import/action, and banned-key cleanup. That is the right place to expose Grok pool health and admin operations.

Sources:
- [`apps/aether-gateway/src/handlers/admin/provider/pool_admin/mod.rs`](/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/pool_admin/mod.rs#L60-L175)
- [`apps/aether-gateway/src/control/route/admin/observability_families.rs`](/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/control/route/admin/observability_families.rs#L220-L316)
- [`apps/aether-gateway/src/control/tests/admin_pool.rs`](/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/control/tests/admin_pool.rs)

### Existing Grok-adjacent naming

Aether already treats `grok` as an alias under its admin API format definitions, and `xai` already appears in the shared external model provider list. There is no dedicated Grok transport family in the code currently indexed here. Because the clarified scope includes image generation/editing, Anthropic messages compatibility, account import, and Grok session material, the integration should introduce a native Grok provider family instead of relying only on generic OpenAI-compatible transport.

Sources:
- [`crates/aether-admin/src/system.rs`](/Volumes/mayrain/workspace/Aether/crates/aether-admin/src/system.rs#L525-L540)
- [`apps/aether-gateway/src/handlers/shared/external_models.rs`](/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/shared/external_models.rs#L1-L19)

## Integration Options

### Option 1: Treat grok2api as a single upstream endpoint

Use grok2api as one OpenAI-compatible endpoint with a single service credential and let grok2api manage its own pool internally.

- Pros: minimal Aether change
- Cons: Aether does not own pool selection, quota visibility, or admin operations

### Option 2: Recommended MVP, native Aether Grok provider family

Model Grok accounts as Aether provider keys, add a Grok provider template/transport module, and let the existing pool stack own the lifecycle:

- provider catalog records for provider, endpoint, and key
- Grok-specific auth config parsing and request shaping
- `openai:chat`, `openai:responses`, `claude:messages`, and `openai:image` API format mapping
- catalog-driven model visibility for chat and image models, with video excluded
- existing candidate selection, health, quota, and admin pool routes
- failure feedback and OAuth/invalid-marker handling through existing status fields

Why this is the right MVP:

- grok2api proves the required non-video protocol surface exists
- Aether already has pool-aware scheduler and admin infrastructure
- no new pool scheduler is needed
- it preserves Aether as the source of truth for account visibility and routing

### Option 3: Generic OpenAI-compatible transport only

Use existing generic OpenAI-compatible transport for chat/responses/image-generation only and defer Grok protocol-specific work.

This is no longer recommended as the main plan because it cannot cover the clarified endpoint scope without quickly growing special cases outside a coherent provider family.

## Recommended MVP Scope

1. Provider family: add `grok` as a fixed provider type/template with typed auth config and redaction.
2. Account pool: represent each Grok account in Aether provider catalog keys, not as a single shared secret.
3. Endpoints: support chat, responses, messages, models, and image generation; video and Aether-owned image upload/cache remain excluded.
4. Model visibility: map Grok chat/image model capability and tier metadata into Aether model records.
5. Admin visibility: expose pool state through the existing admin pool endpoints and provider catalog records.

## Out of Scope for MVP

- Rebuilding grok2api's internal scheduler inside Aether
- Adding a new external dependency just for Grok
- Implementing video-specific Grok routing
- Changing generic Aether pool semantics unless Grok forces a compatibility gap

## Open Questions

- Which Grok auth material should Aether store per key: raw bearer token, OAuth refresh material, or a derived session blob?
- Does Grok2API require per-account refresh semantics that map cleanly onto Aether's existing `oauth_invalid` and quota snapshot fields?
- Can model discovery and allowed-model persistence cover all Grok models, or will some models need manual whitelist handling?
