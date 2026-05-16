# Native Aether Grok Provider Design Research

## Direction

Implement Grok as an Aether-native account-to-API provider family, using:

* Aether's existing fixed provider and provider pool patterns from `codex`, `kiro`, and `antigravity`.
* `grok2api` as the reference for Grok Web protocol behavior.

Do not make `grok2api` the long-term runtime authority for account selection. Aether should own account records, selection, refresh state, admin visibility, usage, and failure feedback.

## Target Scope

In scope:

* `GET /v1/models`
* `POST /v1/chat/completions`
* `POST /v1/responses`
* `POST /v1/messages`
* `POST /v1/images/generations`
* provider account import and pool management

Out of scope:

* all video endpoints and video job state
* Aether-owned image upload/cache persistence or asset-management flows
* running `grok2api` as a sidecar target architecture
* importing arbitrary executable proxy URLs

## Aether Patterns to Reuse

### Fixed Provider Template

Existing fixed providers are registered in `aether-provider-transport` with `provider_type`, base URL, endpoint templates, and runtime policy.

References:

* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/provider_types.rs:149`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/provider_types.rs:255`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/provider_types.rs:295`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/provider_types.rs:342`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/provider_types.rs:395`

Recommended Grok template:

* `provider_type`: `grok`
* base URL: `https://grok.com`
* endpoints:
  * `openai:chat` for chat completions
  * `openai:responses` for responses
  * `claude:messages` only if Aether will expose Anthropic compatibility through conversion
  * `openai:image` for image generation
* runtime policy:
  * fixed provider
  * key inherits API formats
  * model fetch disabled initially unless Aether implements pool-aware model visibility
  * local standard transport disabled for Grok-specific endpoints unless a dedicated Grok transport can classify the request

### Auth Config Resolution

Kiro and Antigravity show the preferred structure:

* parse `key.decrypted_auth_config`,
* reject wrong `provider_type`,
* return explicit unsupported reasons,
* keep dynamic/complex secrets in `auth_config`,
* build provider-specific headers only after validation.

References:

* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/kiro/auth.rs:21`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/kiro/auth.rs:93`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/antigravity/auth.rs:34`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/antigravity/auth.rs:121`

Recommended Grok auth config fields:

* `provider_type: "grok"`
* `sso_token`
* optional `sso_rw_token`
* optional `cf_clearance`
* optional `user_agent`
* optional `account_id`
* optional `account_user_id`
* optional `email`
* optional `pool_tier`: `basic`, `super`, `heavy`
* optional `plan_type`
* optional `expires_at`
* optional `last_refresh_at`

The encrypted `api_key` field can store a placeholder or primary SSO token depending on Aether's existing key contract, but the source of truth should be encrypted `auth_config`.

### Admin Import and Refresh

Aether already has a provider OAuth import path that encrypts `api_key` and `auth_config`, assigns inherited API formats, detects duplicates, and triggers post-import account-state refresh.

References:

* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/oauth/provisioning.rs:57`
* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/oauth/provisioning.rs:95`
* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/import.rs:221`
* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/oauth/runtime.rs:112`

Recommended Grok import behavior:

* Add a Grok-specific batch import parser, closer to Kiro import than Codex OAuth exchange.
* Accept JSON lines or array entries containing session material, account identity hints, tier/plan, and optional proxy node ID.
* Do not call an OAuth token endpoint unless a real Grok refresh flow is proven.
* After import, trigger a Grok account-state refresh that probes rate limits and writes status metadata.

### Quota and Invalid State

Kiro's quota refresh flow is the closest shape:

* force refresh/resolve local auth,
* execute provider-specific quota plan,
* parse response metadata,
* update encrypted auth config if refresh changes tokens,
* mark invalid/banned state on credential failures.

References:

* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/oauth/quota/kiro/mod.rs:88`
* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/oauth/quota/kiro/mod.rs:194`
* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/oauth/quota/kiro/mod.rs:247`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/protocol/xai_usage.py:101`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/account/invalid_credentials.py:16`

Recommended Grok metadata:

* `grok.pool_tier`
* `grok.mode_quotas`
* `grok.is_banned`
* `grok.ban_reason`
* `grok.last_rate_limit_probe_at`
* `grok.clearance_state`

## Transport Modules

Add a `grok` module under `crates/aether-provider-transport/src/`, following the existing public export pattern in `lib.rs`.

References:

* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/lib.rs:1`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/lib.rs:83`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/kiro/request.rs:21`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/antigravity/request.rs:35`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/antigravity/policy.rs:47`

Suggested submodules:

* `grok/auth.rs`: parse auth_config and build SSO/clearance headers.
* `grok/policy.rs`: classify supported Grok request shapes.
* `grok/request.rs`: build Grok app-chat payload from OpenAI/Responses/Anthropic inputs.
* `grok/image.rs`: classify and build image generation requests.
* `grok/url.rs`: centralize Grok URLs copied from `grok2api` endpoint table.
* `grok/usage.rs`: parse rate-limit/quota responses.

## API Format Mapping

Recommended first mapping:

* `openai:chat` -> Grok app-chat
* `openai:responses` -> normalized message input -> Grok app-chat -> Responses-shaped output
* `claude:messages` -> Anthropic input conversion -> Grok app-chat -> Anthropic-shaped output
* `openai:image` -> Grok image generation, selected by request shape/model

Image implementation should be explicit about the two generation modes:

* `grok-imagine-image-lite`: chat-based image generation path
* `grok-imagine-image` / `grok-imagine-image-pro`: `wss://grok.com/ws/imagine/listen`

`grok2api` also implements image edit by uploading references into Grok asset storage, but Aether should not own that upload/cache workflow in this phase. Treat it as reference-only unless a later requirement asks Aether to mediate uploaded assets.

References:

* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:235`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py:266`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py:272`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py:639`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/transport/imagine_ws.py:278`

## Model Registry

Do not rely on a static `/v1/models` dump only. Grok model visibility depends on account pool availability and model tier. For Aether:

* register Grok global/provider models for chat and images,
* associate models with required capabilities and pool tier,
* hide or mark unavailable models when no eligible pool key exists,
* exclude `grok-imagine-video`.

References:

* `/Volumes/mayrain/workspace/private/grok2api/app/control/model/registry.py:12`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/model/spec.py:63`

## Security Guardrails

Required before implementation:

* All session material goes into encrypted provider key `auth_config`.
* Admin pool/key payloads must never return SSO, clearance, or proxy secrets.
* Remote image URL fetch and local image cache behavior from `grok2api` are not copied into Aether.
* Proxy input must be Aether proxy node IDs, not arbitrary URLs imported from `grok2api`.
* Logs must redact `sso`, `sso-rw`, `cf_clearance`, and authorization material.

References:

* `/Volumes/mayrain/workspace/Aether/.trellis/tasks/05-12-grok2api-provider-pool-integration/research/security-ops.md`

## Phased Implementation Order

1. Provider skeleton:
   * add `grok` fixed provider template,
   * add auth_config parser/redaction tests,
   * add admin import parser and encrypted key creation path.

2. Chat and Responses:
   * implement Grok app-chat request conversion,
   * implement SSE/non-stream output conversion,
   * add pool selection and invalid-feedback tests.

3. Models:
   * add Grok model records and capability/tier metadata,
   * exclude video,
   * verify model visibility against available pool state.

4. Anthropic Messages:
   * add `claude:messages` mapping only after chat conversion is stable.

5. Images:
   * add Lite chat-based image generation first,
   * add WebSocket image generation.

6. Operations:
   * add rate-limit/quota probe,
   * add admin pool status metadata,
   * add redaction and SSRF regression tests.

## Verification Plan

* Unit tests for Grok auth_config parsing and unsupported reasons.
* Unit tests for Grok model capability/tier mapping with video excluded.
* Admin import tests proving encrypted auth_config and duplicate detection.
* Scheduler tests proving a Grok request selects a pool key and records selected key metadata.
* Route tests for `openai:chat`, `openai:responses`, `claude:messages`, and `openai:image`.
* Image tests for generation request mapping and remote URL/cache non-ownership.
* Security tests proving admin key payloads and logs redact sensitive session material.
