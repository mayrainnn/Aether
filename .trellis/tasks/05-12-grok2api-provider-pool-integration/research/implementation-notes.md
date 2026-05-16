# Grok Native Provider Implementation Notes

## Implemented Scope

* Added a native `grok` fixed provider family for `openai:chat`, `openai:responses`, `claude:messages`, and `openai:image`.
* Added Grok session/cookie auth helpers in provider transport. Grok accounts are imported as encrypted provider-key auth config and are not treated as bearer OAuth runtime credentials.
* Added Grok request planning for OpenAI chat, OpenAI responses, same-format passthrough, and image-generation requests.
* Added a dedicated local execution runtime adapter that sends Grok app-chat requests and converts Grok SSE-style output back into client-facing OpenAI chat, Responses, Claude messages, or image-generation response bodies.
* Added Grok quota refresh via `/rest/rate-limits`, storing per-mode quota metadata under the provider-key `grok` metadata bucket.
* Wired Grok quota snapshots into catalog payloads, pool quota probe, admin pool quota exhaustion checks, and frontend quota display helpers.
* Added Grok pool-row account quota rendering in the admin payload builder so the pool list can show model-scoped Grok tier text instead of `-` once a quota snapshot is materialized.
* Added Grok account tier inference from live `/rest/rate-limits` totals, following the `grok2api` quota policy: basic is inferred from fast total `30`, super from auto total `50` or fast total `140`, and heavy from auto total `150` or fast total `400`.
* Materialized inferred Grok `plan_type` / `pool_tier` into quota metadata and status snapshots so existing Aether provider-key and pool-list plan badge code can render account level without a Grok-only data path.
* Added value fields (`used_value`, `remaining_value`, `limit_value`) to model-scoped quota windows when upstream metadata contains remaining/total counts, allowing the pool UI to show Grok mode quotas as normal Aether quota progress rows.
* Added a generic `browser_wreq` transport backend to the execution-plan transport profile contract. Grok session-auth keys use this Rust in-process browser impersonation backend so quota refresh, chat/responses/messages, and image transit calls do not depend on plain `reqwest`.
* Removed the earlier Python sidecar transport path; `dev.sh` starts only the Rust gateway.
* Extended Grok account import to preserve `cf_cookies`, `cf_clearance`, `user_agent`, and `browser_profile` in encrypted key auth config. Full pasted browser cookies are split into `sso`/`sso-rw` session credentials plus a cleaned Cloudflare cookie profile so generated Cookie headers do not duplicate session cookies.

## Deliberate Non-Scope

* Video endpoints and video task registration.
* Aether-owned image upload, image cache, asset persistence, or remote media fetching.
* Copying `grok2api` plaintext account storage, admin credential defaults, proxy URL model, or media-cache behavior.
* Implementing browser TLS/HTTP2 impersonation through the Rust `wreq` backend as a reusable transport capability for future browser-fingerprint-sensitive providers.

## Verification

* `cargo check -p aether-gateway`
* `cargo test -p aether-provider-transport grok --lib`
* `cargo test -p aether-gateway grok --lib`
* `cargo test -p aether-gateway maintenance::runtime::pool_quota_probe::tests::parses_quota_updated_at_seconds_and_milliseconds --lib`
* `cargo test -p aether-gateway handlers::shared::catalog::tests::provider_key_status_snapshot_payload_backfills_grok_model_quota --lib`
* `cargo test -p aether-admin provider::pool::tests::detects_grok_exhaustion_only_when_all_modes_are_empty --lib`
* `npm run test:run -- src/features/providers/utils/__tests__/providerTypeUtils.spec.ts src/utils/__tests__/providerKeyQuota.spec.ts src/features/usage/utils/__tests__/poolTrace.spec.ts`
* `npm run type-check`
* `cargo test -p aether-gateway batch::parse::tests --lib`
* `cargo test -p aether-gateway quota::grok::tests --lib`
* `cargo test -p aether-gateway direct_sync_execution_runtime_routes_browser_wreq_transport_in_process --lib`
* `npm run test:run -- src/features/providers/components/__tests__/OAuthAccountDialog.grok-import.spec.ts`
* `npm run test:run -- src/utils/__tests__/providerKeyQuota.spec.ts src/views/admin/__tests__/PoolManagement.codex-cycle-stats.spec.ts`
* `npx eslint src/utils/providerKeyQuota.ts src/utils/__tests__/providerKeyQuota.spec.ts src/views/admin/PoolManagement.vue src/views/admin/__tests__/PoolManagement.codex-cycle-stats.spec.ts src/api/endpoints/types/statusSnapshot.ts src/features/providers/components/ProviderDetailDrawer.vue src/utils/oauthPlanType.ts --quiet`
* Rust browser transport smoke coverage is provided by the in-process `browser_wreq` execution runtime test.

## Risk Notes

* GitNexus marks `sync_provider_key_quota_status_snapshot` and `admin_pool_key_account_quota_exhausted` as CRITICAL because they feed admin payloads and pool scheduling. The implementation only adds `grok` branches and keeps existing provider behavior intact.
* `gitnexus_detect_changes` reports critical risk because the current workspace contains many pre-existing dirty files. The Grok-specific changed flows are expected: provider planning, execution runtime, quota refresh, quota snapshotting, pool scheduling, and frontend provider/quota display.
* The Grok runtime adapter is intentionally a first native bridge for non-video transit behavior. It does not attempt to own Grok media uploads or cache assets inside Aether.
* `cf_clearance` remains browser-profile, User-Agent, and proxy-affinity sensitive. Importing the same Cookie that works in `grok2api` now preserves the required profile material, but successful Grok calls still depend on using a compatible browser profile and the same usable network egress.
