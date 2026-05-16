# Aether Native Provider Patterns

Scope: repository-local research for provider-pool integration.

This note summarizes the native provider patterns already established in Aether and the parts that should be reused before introducing any new provider-pool logic.

## Findings

1. Provider support is split across three layers, not one:
   - `crates/aether-oauth` owns provider OAuth adapters and capability dispatch.
   - `crates/aether-provider-transport` owns runtime policies, fixed-provider templates, auth resolution, and request URL shaping.
   - `crates/aether-model-fetch` and `apps/aether-gateway` own model discovery plus request execution/planning.
   - Evidence: adapter registry and built-in adapters in [`crates/aether-oauth/src/provider/service.rs`](../../../../crates/aether-oauth/src/provider/service.rs#L19-L35), fixed-provider templates and policy routing in [`crates/aether-provider-transport/src/provider_types.rs`](../../../../crates/aether-provider-transport/src/provider_types.rs#L197-L240) and [`crates/aether-provider-transport/src/provider_types.rs`](../../../../crates/aether-provider-transport/src/provider_types.rs#L355-L405), and model-fetch target selection in [`crates/aether-model-fetch/src/logic.rs`](../../../../crates/aether-model-fetch/src/logic.rs#L169-L205).
   - Implication: a new provider-pool integration should not be modeled as a single “provider service”; it will need entries in all three layers if it wants to behave like native providers.

2. `provider_type` is the primary routing key everywhere.
   - Built-in OAuth adapters are resolved by `provider_type`, including `claude_code`, `codex`, `chatgpt_web`, `gemini_cli`, `antigravity`, and `kiro`.
   - The generic OAuth templates are keyed by `provider_type`, and `provider_type` is also used for fixed-provider runtime templates.
   - Evidence: adapter registration and lookup in [`crates/aether-oauth/src/provider/service.rs`](../../../../crates/aether-oauth/src/provider/service.rs#L19-L49), generic OAuth templates in [`crates/aether-oauth/src/provider/providers/generic.rs`](../../../../crates/aether-oauth/src/provider/providers/generic.rs#L29-L100), fixed-provider templates in [`crates/aether-provider-transport/src/provider_types.rs`](../../../../crates/aether-provider-transport/src/provider_types.rs#L242-L353), and admin template lookup in [`crates/aether-provider-transport/src/provider_types.rs`](../../../../crates/aether-provider-transport/src/provider_types.rs#L447-L516).
   - Implication: if `grok2api` is added, it should first be decided whether it is a fixed provider template, a generic OAuth template, or a fully custom adapter.

3. Codex and ChatGPT Web are the canonical example of a thin OAuth wrapper.
   - They reuse the generic OAuth adapter, but Codex adds authorize URL query hints (`prompt=login`, `id_token_add_organizations=true`, `codex_cli_simplified_flow=true`).
   - Their request auth is still bearer-style, and account probing reads metadata only.
   - Evidence: Codex adapter behavior in [`crates/aether-oauth/src/provider/providers/codex.rs`](../../../../crates/aether-oauth/src/provider/providers/codex.rs#L21-L51) and [`crates/aether-oauth/src/provider/providers/codex.rs`](../../../../crates/aether-oauth/src/provider/providers/codex.rs#L98-L105), plus the shared generic template rows in [`crates/aether-oauth/src/provider/providers/generic.rs`](../../../../crates/aether-oauth/src/provider/providers/generic.rs#L42-L65).
   - Implication: if `grok2api` can ride standard OAuth with bearer auth and normal token refresh, the generic template path is the closest fit.

4. Kiro is the clearest example of a fully custom provider.
   - It defines a dedicated `KiroAuthConfig` with machine id, regions, auth method, client credentials, profile ARN, and token fields.
   - It supports both social refresh and IDC refresh flows, with different hosts, headers, and payloads.
   - Evidence: schema and refresh helpers in [`crates/aether-oauth/src/provider/providers/kiro.rs`](../../../../crates/aether-oauth/src/provider/providers/kiro.rs#L17-L33), [`crates/aether-oauth/src/provider/providers/kiro.rs`](../../../../crates/aether-oauth/src/provider/providers/kiro.rs#L166-L259), and [`crates/aether-oauth/src/provider/providers/kiro.rs`](../../../../crates/aether-oauth/src/provider/providers/kiro.rs#L262-L335); mirrored runtime auth resolution in [`crates/aether-provider-transport/src/kiro/auth.rs`](../../../../crates/aether-provider-transport/src/kiro/auth.rs#L37-L60) and [`crates/aether-provider-transport/src/kiro/auth.rs`](../../../../crates/aether-provider-transport/src/kiro/auth.rs#L93-L140).
   - Implication: any provider with nontrivial refresh logic or device identity requirements should be treated like Kiro, not like Codex.

5. The admin import pipeline is split into single-import, batch-import, and refresh-update flows.
   - Single import accepts refresh token or access token, but access-token import is explicitly limited to Codex / ChatGPT Web.
   - Kiro single refresh import is rejected and directed to batch import or device authorization.
   - Batch Kiro import normalizes entries, validates refresh tokens, refreshes them, and then persists catalog keys.
   - Evidence: single-import flow in [`apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/import.rs`](../../../../apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/import.rs#L123-L219) and [`apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/import.rs`](../../../../apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/import.rs#L276-L380); Kiro batch import in [`apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/batch/kiro_import.rs`](../../../../apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/batch/kiro_import.rs#L29-L78) and [`apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/batch/kiro_import.rs`](../../../../apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/batch/kiro_import.rs#L87-L347); Kiro refresh transport in [`apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/kiro.rs`](../../../../apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/kiro.rs#L93-L310).
   - Implication: a provider-pool integration should decide up front whether import happens as a single token exchange, a batch credential ingestion, or a separate device-auth flow.

6. Token payload enrichment is intentionally layered and provider-aware.
   - Admin code constructs `auth_config` from token payloads, preserves `provider_type`, `updated_at`, token metadata, refresh token, and expiry, then enriches identity fields.
   - Codex/ChatGPT Web enrichment pulls claims from OpenAI-style nested auth/profile objects and JWTs.
   - Evidence: `build_provider_oauth_auth_config_from_token_payload` in [`apps/aether-gateway/src/handlers/admin/provider/oauth/provisioning.rs`](../../../../apps/aether-gateway/src/handlers/admin/provider/oauth/provisioning.rs#L57-L93) and identity enrichment in [`crates/aether-admin/src/provider/state.rs`](../../../../crates/aether-admin/src/provider/state.rs#L216-L289).
   - Implication: the auth config should be treated as a normalized provider envelope, not a raw token blob.

7. Transport auth/config absorption is deliberately conservative.
   - The transport layer only absorbs a safe subset of `auth_config` into headers, query params, and path.
   - Sensitive fields such as `access_token`, `refresh_token`, `client_id`, `client_secret`, `private_key`, and similar secrets are blocked.
   - Evidence: safe-subset parser and blocked key lists in [`crates/aether-provider-transport/src/auth_config.rs`](../../../../crates/aether-provider-transport/src/auth_config.rs#L6-L41) and [`crates/aether-provider-transport/src/auth_config.rs`](../../../../crates/aether-provider-transport/src/auth_config.rs#L84-L121), plus parsing restrictions in [`crates/aether-provider-transport/src/auth_config.rs`](../../../../crates/aether-provider-transport/src/auth_config.rs#L141-L260).
   - Implication: provider-pool integration should prefer explicit typed fields over hiding request-critical data in arbitrary auth_config JSON.

8. Kiro and Antigravity show the two major request-shaping patterns: wrapper envelope vs safe passthrough envelope.
   - Kiro converts Claude messages into `conversationState`, adds inference config, preserves profile ARN, and then applies body/header rules.
   - Antigravity requires a safe request body with `contents`, rejects system instructions/tools/thinking/image/function call features, and wraps the request into `project/request/model/userAgent/requestType`.
   - Evidence: Kiro request body/header construction in [`crates/aether-provider-transport/src/kiro/request.rs`](../../../../crates/aether-provider-transport/src/kiro/request.rs#L21-L84) and [`crates/aether-provider-transport/src/kiro/request.rs`](../../../../crates/aether-provider-transport/src/kiro/request.rs#L98-L150); Antigravity request classifier/envelope in [`crates/aether-provider-transport/src/antigravity/request.rs`](../../../../crates/aether-provider-transport/src/antigravity/request.rs#L35-L90) and [`crates/aether-provider-transport/src/antigravity/policy.rs`](../../../../crates/aether-provider-transport/src/antigravity/policy.rs#L47-L116).
   - Implication: if `grok2api` needs a custom request body, it should be classified early as either a transform-heavy envelope or a strict safe-pass-through shape.

9. Request URL routing is provider-specific and sometimes region-aware.
   - Kiro resolves `{region}` placeholders and uses `/generateAssistantResponse`, `/ListAvailableModels`, and `/mcp`.
   - Antigravity routes to `/v1internal:GenerateContent` or `/v1internal:StreamGenerateContent` / fetch models via `/v1internal:fetchAvailableModels`.
   - Evidence: Kiro URL helpers in [`crates/aether-provider-transport/src/kiro/url.rs`](../../../../crates/aether-provider-transport/src/kiro/url.rs#L10-L55) and [`crates/aether-provider-transport/src/kiro/url.rs`](../../../../crates/aether-provider-transport/src/kiro/url.rs#L57-L62); transport hook routing in [`crates/aether-provider-transport/src/request_url/mod.rs`](../../../../crates/aether-provider-transport/src/request_url/mod.rs#L205-L287).
   - Implication: provider-pool integration should not assume one upstream URL per provider; the URL router is part of the contract.

10. Model discovery is a first-class behavior, not an optional extra.
    - Model fetch uses provider-specific transports, but also falls back to preset models when a provider has no supported endpoint.
    - Kiro has its own preset model list, Codex has a preset OpenAI model list, and fixed-provider fetch selection is format-priority driven.
    - Evidence: preset and parser logic in [`crates/aether-model-fetch/src/logic.rs`](../../../../crates/aether-model-fetch/src/logic.rs#L169-L205) and [`crates/aether-model-fetch/src/logic.rs`](../../../../crates/aether-model-fetch/src/logic.rs#L229-L278), plus runtime target selection and persistence in [`apps/aether-gateway/src/model_fetch/runtime.rs`](../../../../apps/aether-gateway/src/model_fetch/runtime.rs#L98-L170) and [`apps/aether-gateway/src/model_fetch/runtime.rs`](../../../../apps/aether-gateway/src/model_fetch/runtime.rs#L223-L347).
    - Implication: a new provider should decide whether it supports live model fetch, preset models, or both.

11. Status and invalid-reason feedback is standardized and reusable.
    - Pool/account state can be resolved from metadata or tagged invalid reasons such as `[ACCOUNT_BLOCK]`, `[OAUTH_EXPIRED]`, `[REQUEST_FAILED]`, and `[REFRESH_FAILED]`.
    - Codex quota refresh uses fallback metadata and marks forbidden / disabled states; Antigravity quota refresh parses `quotaInfo` and records forbidden state; Kiro quota refresh updates quota/state from model usage responses.
    - Evidence: state classification in [`crates/aether-admin/src/provider/status.rs`](../../../../crates/aether-admin/src/provider/status.rs#L14-L27), [`crates/aether-admin/src/provider/status.rs`](../../../../crates/aether-admin/src/provider/status.rs#L273-L403), quota parsers in [`crates/aether-admin/src/provider/quota.rs`](../../../../crates/aether-admin/src/provider/quota.rs#L111-L173) and [`crates/aether-admin/src/provider/quota.rs`](../../../../crates/aether-admin/src/provider/quota.rs#L182-L315).
    - Implication: provider-pool integration should emit tagged invalid reasons instead of free-form strings if it wants to participate cleanly in pool state management.

12. The request conversion layer already encodes the native “special-case if necessary, otherwise generic” pattern.
    - Kiro and Antigravity are only selected when the transport/provider shape justifies it.
    - OpenAI responses planning switches to Kiro-specific payload building and Antigravity-specific envelope construction only when the provider type and format match.
    - Evidence: candidate preparation and special-case routing in [`apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs`](../../../../apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs#L93-L170), [`apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs`](../../../../apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs#L229-L367), and [`apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs`](../../../../apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/decision/request.rs#L527-L669).
    - Implication: the cleanest provider-pool integration path is to keep the generic path generic and only add special cases where the upstream protocol truly diverges.

## Practical Readout for a New Provider-Pool Integration

- Prefer a generic OAuth adapter if the provider is bearer/OAuth shaped and only needs normal token exchange/refresh.
- Add a custom auth config and refresh adapter only if the provider needs device identity, region selection, or split social/IDC flows.
- Add a fixed-provider runtime template if the provider has a stable base URL and endpoint set.
- Add model-fetch support only if there is a real upstream discovery endpoint or a justified preset list.
- Emit structured invalid reasons and quota metadata so the pool can classify accounts without custom ad hoc logic.
