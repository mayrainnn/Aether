# Directory Structure

## Crate Layout

```text
crates/aether-provider-pool/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── capability.rs
    ├── plan.rs
    ├── presets.rs
    ├── provider.rs
    ├── quota.rs
    ├── quota_refresh.rs
    ├── service.rs
    └── providers/
        ├── mod.rs
        ├── antigravity.rs
        ├── chatgpt_web.rs
        ├── codex.rs
        ├── default.rs
        ├── kiro.rs
        └── unsupported.rs
```

## Module Ownership

- `lib.rs`: public package surface and crate-local unit tests. Keep cross-crate API decisions visible here.
- `capability.rs`: small capability model. `ProviderPoolCapability` is the enum key; `ProviderPoolCapabilities` is the boolean set.
- `provider.rs`: `ProviderPoolAdapter` trait, `ProviderPoolMemberInput`, and shared endpoint matching helpers.
- `service.rs`: `ProviderPoolService` registry and adapter lookup. This is the only built-in adapter registration point.
- `plan.rs`: provider-aware plan tier derivation and normalization.
- `presets.rs`: scheduling preset normalization plus admin presets payload.
- `quota.rs`: quota snapshot, quota metadata, quota exhaustion, scheduling label, and JSON coercion helpers.
- `quota_refresh.rs`: transport-neutral `ProviderPoolQuotaRequestSpec`.
- `providers/*.rs`: provider-specific adapter implementations and request-spec builders.

## Provider Adapter Pattern

The trait lives in `provider.rs`; implementations live in `providers/*.rs`. A provider file may own:

- `ProviderPoolAdapter` implementation.
- Capability declaration.
- Default scheduling presets.
- Quota endpoint selection.
- Provider-specific quota exhaustion fallback from `upstream_metadata`.
- A transport-neutral `ProviderPoolQuotaRequestSpec` builder.
- Small metadata normalizers that are tightly coupled to that provider.

Example from `crates/aether-provider-pool/src/providers/codex.rs`:

```rust
impl ProviderPoolAdapter for CodexProviderPoolAdapter {
    fn provider_type(&self) -> &'static str { "codex" }

    fn capabilities(&self) -> ProviderPoolCapabilities {
        ProviderPoolCapabilities {
            plan_tier: true,
            quota_reset: true,
            quota_refresh: true,
        }
    }

    fn default_scheduling_presets(&self) -> Vec<PoolSchedulingPreset> {
        vec![PoolSchedulingPreset {
            preset: "recent_refresh".to_string(),
            enabled: true,
            mode: None,
        }]
    }
}
```

## Adding A New Provider Adapter

Use this sequence:

1. Add `src/providers/<provider>.rs`.
2. Implement `ProviderPoolAdapter`.
3. Add provider-specific request spec builders only if quota refresh is supported.
4. Re-export the adapter and builders from `src/providers/mod.rs`.
5. Register the adapter in `ProviderPoolService::with_builtin_adapters()` in `service.rs`.
6. Re-export public symbols from `lib.rs` only when used outside the crate.
7. Add unit tests in `lib.rs` for registration, capabilities, endpoint selection, quota request shape, and quota exhaustion behavior.
8. Check gateway consumers before changing any shared request spec fields.

Do not add the provider only to `providers/mod.rs`; unregistered adapters are invisible to `ProviderPoolService` and admin/gateway callers.

## Fallback Adapters

`DefaultProviderPoolAdapter` in `providers/default.rs` is the unknown-provider fallback and should stay minimal.

`UnsupportedQuotaProviderPoolAdapter` in `providers/unsupported.rs` is used for known providers that need registry presence and tailored quota-refresh messages but do not support automatic quota refresh:

- `claude_code`
- `gemini_cli`
- `vertex_ai`

Prefer this adapter when the provider is known but quota refresh cannot be safely supported.
