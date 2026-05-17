# Quality Guidelines

## Adapter Discipline

- Keep `ProviderPoolAdapter` small and total. Prefer default methods over broad required methods.
- Put provider-specific logic in `providers/<provider>.rs`.
- Use `ProviderPoolService` for lookup; do not duplicate ad hoc provider-type matching in consumers.
- Register every built-in adapter through `ProviderPoolService::with_builtin_adapters()`.
- Keep unknown-provider behavior in `DefaultProviderPoolAdapter`.
- Use `UnsupportedQuotaProviderPoolAdapter` for known providers that need custom unsupported messages.

## Capability Discipline

Capabilities drive public behavior. Do not infer support from provider names in callers.

Correct pattern:

```rust
// crates/aether-provider-pool/src/service.rs
pub fn provider_types_for_capability(&self, capability: ProviderPoolCapability) -> Vec<String> {
    self.adapters
        .iter()
        .filter(|(_, adapter)| adapter.capabilities().supports(capability))
        .map(|(provider_type, _)| provider_type.clone())
        .collect()
}
```

`presets.rs` uses this rule so plan-aware presets are exposed only for providers that support `PlanTier`, and `recent_refresh` only for providers that support `QuotaReset`.

## Test Patterns

Unit tests live in `crates/aether-provider-pool/src/lib.rs`. Follow the current style:

- Build sample `StoredProviderCatalogKey` values with helper functions.
- Assert the exact built-in provider list and fallback behavior.
- Assert quota-refresh support and unsupported messages.
- Assert request spec shape: method, URL, headers, model name, API formats, and cert behavior.
- Assert metadata enrichment and normalization.
- Assert scheduling preset capability filtering and injected defaults.
- Assert quota exhaustion decisions by provider metadata shape.
- Assert plan tier derivation from quota snapshot and provider metadata.

Examples already covered:

- `builtin_service_registers_provider_pool_adapters`
- `builtin_service_owns_quota_refresh_support_and_endpoint_selection`
- `codex_quota_request_uses_wham_usage_endpoint`
- `kiro_quota_request_includes_profile_arn_when_present`
- `chatgpt_web_quota_metadata_enriches_auth_and_normalizes_free_limit`
- `preset_payload_derives_provider_support_from_capabilities`
- `codex_adapter_injects_recent_refresh_and_filters_by_capability`
- `provider_quota_exhaustion_is_adapter_owned`

## Anti-Patterns

Do not:

- Add HTTP clients, request execution, retries, timeout policy, or proxy logic to this crate.
- Add database queries, Redis reads, runtime lease state, or persistence.
- Add Axum handlers or admin response construction beyond neutral JSON payload helpers.
- Hard-code provider names in gateway callers when capability-based lookup is available.
- Extend `DefaultProviderPoolAdapter` with real provider behavior.
- Add provider-specific branches to `plan.rs`, `presets.rs`, or `quota.rs` when a provider adapter override is the cleaner boundary.
- Return invalid quota request specs and expect gateway execution to fail later.
- Change `ProviderPoolAdapter` without checking all implementations and gateway consumers.

## Add New Provider vs Extend Existing Provider

Add a new provider adapter when:

- The provider type string is new in the provider catalog.
- Quota refresh uses a different endpoint format, URL, headers, body, or provider API format.
- Quota exhaustion depends on a different metadata bucket shape.
- Unsupported quota behavior needs a provider-specific message.

Extend an existing adapter when:

- The provider type is unchanged.
- The quota endpoint is the same but needs small header/body metadata additions.
- Metadata normalization can be done without changing capability semantics.
- Tests can prove compatibility with existing request specs and quota exhaustion behavior.

## Required Verification

For any code change in this crate, run:

```bash
cargo test -p aether-provider-pool
```

For documentation-only spec edits, at minimum verify:

- No unfinished markers remain in `.trellis/spec/aether-provider-pool/backend`.

Use targeted gateway tests when changing public API or behavior consumed by admin pool routes, pool scheduler, or quota refresh handlers.
