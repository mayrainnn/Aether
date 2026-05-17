# Error Handling

## Trait Semantics

`ProviderPoolAdapter` is not a `Result`-returning trait. Its methods use deterministic defaults:

- `capabilities()` defaults to no capabilities.
- `default_scheduling_presets()` defaults to an empty vector.
- `supports_quota_refresh()` is derived from `ProviderPoolCapability::QuotaRefresh`.
- `quota_refresh_endpoint(...)` returns `None` when unsupported or no matching endpoint exists.
- `quota_refresh_unsupported_message()` and `quota_refresh_missing_endpoint_message()` return caller-facing strings.
- `quota_exhausted(...)` returns `false` when no trusted decision can be derived.

This keeps adapter lookup and scheduling signal construction total. Callers must treat missing endpoint/support as a normal state, not an exception.

## Where Failures Surface

Transport execution is outside this crate. The crate builds `ProviderPoolQuotaRequestSpec`; gateway code executes it and persists status.

Known caller boundary:

- `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/shared.rs` wraps `ProviderPoolService` for support checks, endpoint selection, and messages.
- `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/codex/plan.rs` uses `build_codex_pool_quota_request`.
- `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/kiro/plan.rs` uses `build_kiro_pool_quota_request`.
- `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/chatgpt_web.rs` uses ChatGPT Web request and metadata helpers.
- `apps/aether-gateway/src/handlers/admin/provider/oauth/quota/antigravity.rs` uses the Antigravity request builder.

Do not move HTTP execution, retry policy, timeout handling, proxy handling, or persistence into `aether-provider-pool`.

## Request Builder Errors

Most request builders return `ProviderPoolQuotaRequestSpec` directly because all required inputs are already resolved by the caller.

Codex is the exception:

```rust
// crates/aether-provider-pool/src/providers/codex.rs
pub fn build_codex_pool_quota_request(...) -> Result<ProviderPoolQuotaRequestSpec, String>
```

It returns `Err(String)` when OAuth credentials are missing and `decrypted_api_key` is empty or the sentinel marker. Keep this error local to request-spec construction; downstream execution should not receive an invalid quota request.

## Validation In plan.rs

`derive_plan_tier` returns `Option<String>`.

Rules from `crates/aether-provider-pool/src/plan.rs`:

- Plan tier derivation only runs for auth-managed keys.
- Auth-managed means OAuth, or Kiro bearer with auth config.
- Reads quota snapshot first, then provider-specific `upstream_metadata`, then the raw `auth_config`.
- Candidate fields are `plan_type`, `tier`, `plan`, `subscription_title`, and `subscription_plan`.
- `normalize_provider_plan_tier` trims whitespace, strips a provider prefix like `codex:Plus`, lowercases, and returns `None` for empty output.

Do not silently invent a plan tier for non-auth-managed keys.

## Validation In presets.rs

`normalize_provider_scheduling_presets` is a filter and normalizer:

- Trims and lowercases preset names.
- Drops empty names.
- Drops duplicates, preserving first occurrence.
- Drops capability-gated presets when the adapter does not support them.
- Adds adapter default presets only when the admin input had at least one valid entry.
- Keeps only the first enabled distribution-mode preset from `lru`, `cache_affinity`, `load_balance`, and `single_account`.
- Omits `lru` from normalized output because it is the implicit default distribution mode.
- Preserves strategy preset order after distribution normalization.

Anti-pattern: do not preserve invalid presets for UI visibility. The admin payload in `build_admin_pool_scheduling_presets_payload` is the source of discoverable options; normalized scheduling config is the executable subset.
