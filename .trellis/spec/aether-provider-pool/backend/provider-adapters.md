# Provider Adapters

## Capability Rules

Capabilities are explicit and opt-in:

```rust
// crates/aether-provider-pool/src/capability.rs
pub enum ProviderPoolCapability {
    PlanTier,
    QuotaReset,
    QuotaRefresh,
}
```

Only adapters that set `plan_tier`, `quota_reset`, or `quota_refresh` to `true` should be treated as supporting those features. `presets.rs` uses capabilities to expose plan-aware presets (`free_first`, `plus_first`, `pro_first`, `team_first`) and reset-aware `recent_refresh`.

## Built-In Providers

| Provider type | Adapter | Capabilities | Quota refresh endpoint |
| --- | --- | --- | --- |
| `codex` | `CodexProviderPoolAdapter` | PlanTier, QuotaReset, QuotaRefresh | active `openai:responses` endpoint |
| `kiro` | `KiroProviderPoolAdapter` | PlanTier, QuotaReset, QuotaRefresh | active `claude:messages`, fallback to any endpoint |
| `chatgpt_web` | `ChatGptWebProviderPoolAdapter` | QuotaRefresh | active `openai:image` endpoint |
| `antigravity` | `AntigravityProviderPoolAdapter` | QuotaRefresh | active `gemini:generate_content` endpoint |
| `claude_code` | `UnsupportedQuotaProviderPoolAdapter` | none | unsupported message |
| `gemini_cli` | `UnsupportedQuotaProviderPoolAdapter` | none | unsupported message |
| `vertex_ai` | `UnsupportedQuotaProviderPoolAdapter` | none | unsupported message |
| unknown provider | `DefaultProviderPoolAdapter` | none | generic unsupported behavior |

## Codex

Source: `crates/aether-provider-pool/src/providers/codex.rs`.

Codex supports plan tier, quota reset, and quota refresh. It injects the default `recent_refresh` scheduling preset and builds a WHAM usage request:

```rust
pub const CODEX_WHAM_USAGE_URL: &str =
    "https://chatgpt.com/backend-api/wham/usage";
```

Request behavior:

- Uses a resolved OAuth header when available.
- Falls back to `decrypted_api_key` as `authorization: Bearer ...`.
- Returns `Err(String)` when OAuth/API key material is missing or only the sentinel token is present.
- Adds `chatgpt-account-id` for non-free accounts with `account_id`.
- Uses `client_api_format` and `provider_api_format` of `openai:responses`.

Quota exhaustion fallback reads the provider metadata bucket:

- `credits_unlimited == true` means not exhausted.
- No window data plus `has_credits == false` means exhausted.
- `primary_used_percent >= 100` or `secondary_used_percent >= 100` means exhausted.

## Kiro

Source: `crates/aether-provider-pool/src/providers/kiro.rs`.

Kiro supports plan tier, quota reset, and quota refresh. It builds a GET request to:

```rust
pub const KIRO_USAGE_LIMITS_PATH: &str = "/getUsageLimits";
```

Request behavior:

- Host is `q.<api_region>.amazonaws.com`, defaulting empty region to `us-east-1`.
- Empty Kiro version defaults to `0.3.210`.
- Adds AWS SDK user-agent headers and a random `amz-sdk-invocation-id`.
- Adds `profileArn` to the query string when present.
- Uses `client_api_format = "claude:messages"` and `provider_api_format = "kiro:usage"`.

Quota exhaustion fallback treats any of these as exhausted:

- `remaining <= 0`.
- `usage_percentage >= 100`.
- `current_usage >= usage_limit` when `usage_limit > 0`.

## ChatGPT Web

Source: `crates/aether-provider-pool/src/providers/chatgpt_web.rs`.

ChatGPT Web supports quota refresh only. It intentionally does not declare `PlanTier`, even though metadata enrichment may copy `plan_type` from auth config for display and quota calculations.

Request behavior:

- Empty endpoint base URL defaults to `https://chatgpt.com`.
- Builds a POST to `/backend-api/conversation/init`.
- Sends browser-like headers, generated `oai-device-id`, generated `oai-session-id`, and `system_hints: ["picture_v2"]`.
- Uses `client_api_format = "openai:image"` and `provider_api_format = "chatgpt_web:conversation_init"`.
- Sets `accept_invalid_certs = true`.

Metadata behavior:

- `enrich_chatgpt_web_quota_metadata` copies plan/email/account identifiers from auth config when missing.
- `normalize_chatgpt_web_image_quota_limit` sets free-plan image total to `25.0`, preserves existing paid limits, and derives `image_quota_used` from remaining quota where possible.

Quota exhaustion fallback treats `image_quota_blocked == true`, `image_quota_remaining <= 0`, or `image_quota_used >= image_quota_total` as exhausted.

## Antigravity

Source: `crates/aether-provider-pool/src/providers/antigravity.rs`.

Antigravity supports quota refresh only. It builds a POST request to:

```rust
pub const ANTIGRAVITY_FETCH_AVAILABLE_MODELS_PATH: &str =
    "/v1internal:fetchAvailableModels";
```

Request behavior:

- Reuses caller-provided identity headers.
- Inserts authorization, content type, accept, and a default `user-agent` if absent.
- Sends JSON body `{ "project": project_id }`.
- Uses `client_api_format = "gemini:generate_content"` and `provider_api_format = "antigravity:fetch_available_models"`.

It does not override quota exhaustion or plan tier behavior.

## Unsupported And Default

Source: `crates/aether-provider-pool/src/providers/unsupported.rs` and `default.rs`.

Use `UnsupportedQuotaProviderPoolAdapter` for known provider types with specific quota-refresh unsupported messages. Current constants cover:

- `CLAUDE_CODE_PROVIDER_POOL_ADAPTER`
- `GEMINI_CLI_PROVIDER_POOL_ADAPTER`
- `VERTEX_AI_PROVIDER_POOL_ADAPTER`

Use `DefaultProviderPoolAdapter` only as the service fallback for unknown provider strings. It should not accumulate provider-specific behavior.
