# aether-provider-pool Backend Spec

## Package Summary

`crates/aether-provider-pool` is a pure Rust domain crate for provider-specific pool behavior. Its `Cargo.toml` describes it as "Provider-specific pool behavior adapters for Aether" and keeps the dependency surface intentionally small:

- Internal contracts: `aether-data-contracts` for `StoredProviderCatalogKey` and `StoredProviderCatalogEndpoint`.
- Scheduling contract: `aether-pool-core` for `PoolSchedulingPreset` and `PoolMemberSignals`.
- Utility dependencies: `serde_json`, `url`, and `uuid`.

This crate must stay free of HTTP clients, database access, runtime state, and Axum handlers. It builds request specs and derives adapter-owned signals; `apps/aether-gateway` executes I/O and persistence.

## Public API Surface

`crates/aether-provider-pool/src/lib.rs` is the public API boundary. It re-exports:

- Capabilities: `ProviderPoolCapability`, `ProviderPoolCapabilities`.
- Plan helpers: `derive_plan_tier`, `derive_oauth_plan_type`, `normalize_provider_plan_tier`.
- Scheduling helpers: `normalize_provider_scheduling_presets`, `build_admin_pool_scheduling_presets_payload`.
- Adapter contract: `ProviderPoolAdapter`, `ProviderPoolMemberInput`.
- Provider adapters and quota request builders from `providers/*`.
- Quota helpers: `provider_pool_key_account_quota_exhausted`, `provider_pool_member_quota_snapshot`, timestamp helpers, quota metadata provider detection, and scheduling labels.
- Request contract: `ProviderPoolQuotaRequestSpec`.
- Registry: `ProviderPoolService`.

Keep new public functions behind `lib.rs` re-exports only when they are used outside this crate or intentionally part of the package contract.

## Adapter Pattern

The core pattern is:

```rust
// crates/aether-provider-pool/src/provider.rs
pub trait ProviderPoolAdapter: Send + Sync {
    fn provider_type(&self) -> &'static str;
    fn capabilities(&self) -> ProviderPoolCapabilities { ... }
    fn quota_refresh_endpoint(...) -> Option<StoredProviderCatalogEndpoint> { ... }
    fn member_signals(&self, input: &ProviderPoolMemberInput<'_>) -> PoolMemberSignals { ... }
    fn quota_exhausted(&self, input: &ProviderPoolMemberInput<'_>) -> bool { ... }
}
```

`ProviderPoolService` owns adapter registration with `with_builtin_adapters()` in `crates/aether-provider-pool/src/service.rs`. It lowercases provider types, returns `DefaultProviderPoolAdapter` for unknown types, and delegates quota refresh support, endpoint selection, preset normalization, and member signal derivation to the selected adapter.

Built-in provider types registered by tests in `lib.rs`:

- `antigravity`
- `chatgpt_web`
- `claude_code`
- `codex`
- `gemini_cli`
- `kiro`
- `vertex_ai`

GitNexus impact analysis for `ProviderPoolAdapter` shows MEDIUM risk with six direct implementations: `AntigravityProviderPoolAdapter`, `ChatGptWebProviderPoolAdapter`, `CodexProviderPoolAdapter`, `DefaultProviderPoolAdapter`, `KiroProviderPoolAdapter`, and `UnsupportedQuotaProviderPoolAdapter`. Treat trait changes as cross-provider changes.

## Known Consumers

GitNexus cross-file analysis shows these gateway consumers call into the crate:

- `apps/aether-gateway/src/dispatch/pool_scheduler.rs` calls `ProviderPoolService::member_signals` and `normalize_scheduling_presets`.
- `apps/aether-gateway/src/handlers/admin/provider/pool_admin/payloads.rs` and `selection.rs` call `derive_plan_tier`.
- `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/shared.rs` calls `ProviderPoolService` for quota refresh support, endpoint selection, missing endpoint messages, and metadata provider detection.
- Provider quota handlers call the provider-specific builders:
  `codex/plan.rs`, `kiro/plan.rs`, `chatgpt_web.rs`, and `antigravity.rs`.
- `crates/aether-admin/src/provider/pool.rs` wraps pool helpers, including account quota exhaustion and the scheduling presets payload.
- `apps/aether-gateway/src/maintenance/runtime/pool_quota_probe.rs` uses `provider_pool_quota_metadata_updated_at`.

## Guidelines Index

| File | Purpose |
| --- | --- |
| `directory-structure.md` | Crate layout, module ownership, and provider adapter expansion rules. |
| `provider-adapters.md` | Per-provider capability, quota refresh, quota exhaustion, and plan behavior. |
| `error-handling.md` | Adapter failure semantics, validation behavior, and caller-facing error boundaries. |
| `quality-guidelines.md` | Coding discipline, tests, anti-patterns, and when to add or extend adapters. |

## Pre-Development Checklist

Before touching this crate:

- Read `provider.rs` and `service.rs` to confirm whether the change belongs in the trait, registry, or a provider adapter.
- Run GitNexus impact analysis before editing any symbol, especially `ProviderPoolAdapter`, `ProviderPoolService`, or public helper functions.
- Check `providers/mod.rs` and `lib.rs` re-exports if adding public provider behavior.
- Check gateway consumers above before changing request specs, endpoint selection, plan tier output, or scheduling preset semantics.
- Add or update unit tests in `crates/aether-provider-pool/src/lib.rs`.
- Keep network execution and persistence in `apps/aether-gateway`, not in this crate.

## Quality Gate

Run:

```bash
cargo test -p aether-provider-pool
```

If the change affects gateway integration, also run the smallest relevant gateway tests around provider quota refresh, pool scheduler, or admin pool payloads.
