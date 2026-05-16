# brainstorm: integrate grok2api provider pool

## Goal

Research how to make Aether natively convert Grok accounts into API access, with account-pool based management, full non-video endpoint support, and image generation. Video and Aether-owned image upload/cache workflows are out of scope.

## What I Already Know

* User wants a research pass first, using Trellis-related workflow and subagents.
* Aether branch `aether-rust-pioneer` is clean and has no missing commits from `origin/aether-rust-pioneer`; it is ahead of origin by 4 commits and matches `fork/aether-rust-pioneer`.
* Reference project path: `/Volumes/mayrain/workspace/private/grok2api`.
* The target shape is Aether-native Grok account-to-API integration, not a long-lived sidecar wrapper.
* Video is explicitly excluded; chat, responses, messages, models, and image generation remain in scope. Aether is a transit layer, so image upload/cache handling is not an Aether-owned workflow in this phase.
* Existing Aether examples to reuse include `codex`, `kiro`, and `antigravity`.

## Assumptions (Current)

* The first milestone moved from research-only into native implementation after the scope was clarified.
* "grok 2 api" means the `grok2api` project's non-video Grok protocol and API conversion behavior, used as the reference implementation for Grok protocol shape.
* Aether should own scheduling, account selection, health, admin visibility, and model exposure through its existing pool system where possible.

## Open Questions

* Which Grok auth material should Aether store per account, and which parts need native refresh versus imported session-only support?

## Requirements (Evolving)

* Identify `grok2api` non-video API shape: `/v1/chat/completions`, `/v1/responses`, `/v1/messages`, `/v1/models`, `/v1/images/generations`, account selection, refresh, feedback, upstream endpoints, and auth materials. Image upload/cache behavior is reference-only and not an Aether-owned workflow.
* Identify Aether's native provider patterns for `codex`, `kiro`, and `antigravity`, especially auth_config, refresh/import, provider_type routing, transport modules, and pool feedback.
* Propose a native Aether Grok provider family that keeps account selection in Aether and uses `grok2api` only as a protocol reference.
* Call out security, operational, image pipeline, and verification risks before implementation.

## Acceptance Criteria (Evolving)

* [x] Research artifacts exist under `research/` for Aether native provider patterns and `grok2api` non-video capabilities.
* [x] PRD is updated with a native Grok-provider direction, clear scope, and recommended MVP.
* [x] Aether has a fixed `grok` provider family with chat, responses, Claude messages, and image-generation API formats.
* [x] Grok account/session imports are represented as encrypted provider-key auth config and kept non-bearer at runtime.
* [x] Grok candidates can be planned through Aether local transport and executed through a dedicated Grok runtime adapter.
* [x] Grok quota refresh and pool quota probe feed provider-key status snapshots used by admin views and pool scheduling.
* [x] Video and Aether-owned image upload/cache/storage workflows remain out of scope.

## Definition of Done

* Research artifacts written to task files.
* Concrete MVP scope and out-of-scope items recorded.
* Implementation entry points and verification strategy identified.
* Native Grok provider/pool implementation compiles and has targeted tests for transport, runtime adapter, quota, and frontend quota display helpers.

## Research Outcome

## Research References

* [`research/aether-native-provider-patterns.md`](research/aether-native-provider-patterns.md) - Codex, Kiro, and Antigravity native provider patterns to reuse.
* [`research/grok2api-non-video-capabilities.md`](research/grok2api-non-video-capabilities.md) - Grok non-video API, image generation, account pool, and protocol shape.
* [`research/grok-native-provider-design.md`](research/grok-native-provider-design.md) - native Aether Grok design and phased scope.
* [`research/aether-pool-architecture.md`](research/aether-pool-architecture.md) - Aether provider catalog, transport snapshot, scheduler, and admin pool entry points.
* [`research/integration-options.md`](research/integration-options.md) - feasible integration approaches and recommendation.
* [`research/security-ops.md`](research/security-ops.md) - secret handling, SSRF/media fetch risks, admin auth risks, and required guardrails.

### Recommended MVP

* Add a native Grok provider family in Aether using the same structural patterns as `codex`, `kiro`, and `antigravity`.
* Keep Grok account selection, health, quota, and admin visibility in Aether's provider catalog and pool scheduler.
* Support non-video Grok endpoints first: chat, responses, messages, models, and image generation.
* Use `grok2api` as the protocol reference for upstream Grok behavior and image generation flows.
* Store Grok session material only through Aether's encrypted provider key `auth_config`; never mirror `grok2api` plaintext account/config stores.

### Feasible Implementation Options

1. **Native Grok provider family** - add a provider type/template and transport layer in Aether for Grok non-video endpoints.
   * Best match for the user's requirement.
   * Reuses the same provider/key/scheduler/admin shape as existing native providers.

2. **Aether-owned Grok pool only through existing generic OpenAI-compatible transport** - useful only if the required Grok endpoints can be expressed through current OpenAI-compatible routes without Aether-owned upload/cache handling.
   * Lower initial surface area.
   * Risky if image/edit or model mapping cannot be expressed cleanly.

3. **Sidecar wrapper** - keep `grok2api` as a separate service and point Aether at it.
   * Fastest to prototype.
   * Not the target architecture because it leaves account authority outside Aether.

## Out of Scope

* Implementing the provider integration in this initial research pass.
* Adding new third-party dependencies before the design is accepted.
* Video endpoints, video jobs, and video model registration.
* Aether-owned image upload/cache management.
* Changing account pool scheduling behavior unless research proves it is necessary.

## Technical Notes

* Aether workspace: `/Volumes/mayrain/workspace/Aether`
* Reference project: `/Volumes/mayrain/workspace/private/grok2api`
* Relevant Trellis specs and GitNexus impact analysis must be loaded before any later code edits.
* Security review marks direct copying of `grok2api` admin/auth/media-fetch patterns as unsafe; Aether should proxy the supported request surface without taking ownership of image upload/cache persistence.
* Implementation notes and verification evidence are recorded in [`research/implementation-notes.md`](research/implementation-notes.md).
