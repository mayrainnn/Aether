# Grok Account Import Dialog Alignment

## Goal

Align Grok account import with Aether's existing account-provider import UX while preserving Grok-specific semantics. Grok should reuse the shared account import dialog and batch import task pipeline, but the UI must describe Grok session/sso token import rather than web OAuth authorization.

## What I Already Know

- Aether splits providers into key-managed and account-managed providers.
- Account-managed providers use `OAuthAccountDialog.vue`.
- `OAuthAccountDialog.vue` already supports single import, multi-line token import, JSON arrays, JSON Lines, file import, async batch import tasks, progress, and error samples.
- Grok is already classified as an account-managed provider.
- Grok has no normal OAuth authorization template in the provider template table.
- Grok2API imports Grok accounts by token/session material, not by an OAuth browser callback.
- User confirmed Grok should retain batch import if other Aether account providers support batch import.
- User clarified `basic/super/heavy` are account plan/capability traits, not routing pools.

## Requirements

- Reuse the existing shared account import dialog instead of creating a separate Grok-only dialog.
- For Grok, default to the import tab and avoid presenting an unusable web OAuth flow.
- Use Grok-specific labels and help text: session token / sso token / account import.
- Preserve existing batch import support for Grok.
- Accept Grok-friendly fields in frontend parsing for single and batch imports where applicable:
  - `sso_token` / `ssoToken`
  - `token`
  - full browser cookie strings containing `sso`
  - `access_token` / `accessToken`
  - `plan_type` / `planType`
  - `pool_tier` / `poolTier` / `tier`
  - `sso-rw`, `cf_clearance`, and `x-userid` when present in a pasted cookie.
  - optional account identity fields already supported by Aether.
- Treat plan/tier as account metadata for scheduling and display, not as a user-selected route pool.
- Do not add new dependencies.
- Keep changes scoped and compatible with existing Codex/Kiro/Antigravity/Gemini/Claude account import behavior.

## Acceptance Criteria

- [x] Grok "添加账号" opens the shared account dialog in import mode.
- [x] Grok does not show or auto-start the unsupported generic OAuth authorization flow.
- [x] Grok import copy mentions Grok session/sso token and supports batch import.
- [x] Single Grok JSON import can carry `sso_token`, `token`, `pool_tier`, and related account metadata.
- [x] Grok import can extract `sso`, `sso-rw`, `cf_clearance`, and `x-userid` from a pasted browser cookie string.
- [x] Existing non-Grok account import behavior remains unchanged.
- [x] Focused frontend tests pass.
- [x] Focused backend import parser/token import tests pass if backend code changes.

## Out of Scope

- Video.
- Aether-owned image upload/cache/asset persistence.
- New Grok browser-login automation.
- Replacing the shared account dialog with a new provider-specific dialog.

## Technical Notes

- Frontend shared dialog: `frontend/src/features/providers/components/OAuthAccountDialog.vue`.
- Provider type classification: `frontend/src/features/providers/utils/providerTypeUtils.ts`.
- Provider detail entry: `frontend/src/features/providers/components/ProviderDetailDrawer.vue`.
- Pool entry: `frontend/src/views/admin/PoolManagement.vue`.
- Backend single import: `apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/import.rs`.
- Backend batch parser: `apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/batch/parse.rs`.
- Backend Grok token normalization: `apps/aether-gateway/src/handlers/admin/provider/oauth/dispatch/token_import.rs`.
