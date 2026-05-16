# Grok2API Integration Options

## Summary

The target design is now clear:

* `grok2api` is the protocol reference for Grok Web behavior, not the long-term control plane.
* Aether already has the provider catalog, encrypted provider key auth config, pool-aware candidate selection, admin pool routes, model fetch, usage, and health/state plumbing needed to own the Grok account pool itself.

The main design risk is still double ownership. A sidecar wrapper is the fastest compatibility probe, but it should remain a reference-only fallback because it keeps account selection and pool truth outside Aether.

## Option 1: Single Grok2API Sidecar Provider

Run `grok2api` as an upstream OpenAI-compatible endpoint and add it to Aether as one provider endpoint with one service credential.

Pros:

* Lowest implementation cost.
* Reuses `grok2api` protocol coverage for chat, responses, images, video, and Anthropic messages.
* Useful as a short-lived compatibility probe.

Cons:

* Does not meet the stated requirement that Aether manage accounts through its pool.
* Aether cannot see per-account health, quota, invalid credential state, or selection reasons.
* Creates a second scheduler and admin surface.

Use only as a smoke-test or fallback, not as the target architecture.

## Option 2: Recommended MVP - Native Aether Grok Provider Family

Add a first-class Grok provider family in Aether and let it own account import, selection, health, quota, and admin visibility.

MVP shape:

* Add a Grok provider template and transport adapter in Aether.
* Store each Grok account/session as an encrypted provider key with structured auth config.
* Let existing pool candidate selection and admin pool routes own routing, health, and visibility.
* Support the non-video Grok surface first: chat, responses, messages, models, and image generation.
* Use `grok2api` as protocol reference for Grok Web behavior and upstream endpoint shape, while keeping file/upload/cache behavior out of Aether ownership.

Pros:

* Matches the user's account-pool requirement.
* Keeps account truth, selection, and admin state inside Aether.
* Reuses Aether's provider catalog, scheduler, pool admin, and usage accounting.

Cons:

* Requires mapping Grok auth/session material into Aether's key auth config.
* Requires Grok-specific transport and request-shaping support.
* Does not include video or Aether-owned image upload/cache handling.

## Option 3: Sidecar Wrapper

Keep `grok2api` as a separate upstream endpoint and point Aether at it with one service credential.

Pros:

* Lowest implementation cost.
* Useful as a smoke-test or stopgap if native Grok transport proves too large for the first milestone.

Cons:

* Does not let Aether own the Grok pool.
* Creates a second scheduler/control plane if not carefully constrained.
* Not the target architecture.

## Recommendation

Proceed in two phases:

1. **Design/validation phase:** model Grok as an Aether-owned native provider family. Verify chat/responses/messages/image-generation routing, per-account selection, failure feedback, model discovery, and admin visibility.
2. **Fallback phase, only if needed:** keep a sidecar wrapper available for compatibility comparison, but do not let it become the source of truth.

Do not make `grok2api` and Aether both responsible for the same account pool in the target design.

## Verification Plan

* Unit tests for auth config parsing and redaction.
* Scheduler tests proving one request selects a specific Grok provider key through the pool path.
* Route tests for `/v1/chat/completions`, `/v1/responses`, `/v1/messages`, and image-generation routes using mocked upstream responses.
* Admin pool tests proving key list/status/scheduling output includes Grok account state without raw secrets.
* Model fetch tests for `/v1/models` mapping into provider/global model records.
* Security tests proving Aether does not take ownership of remote image fetch/cache behavior.

## Key References

* `/Volumes/mayrain/workspace/private/grok2api/README.md`
* `/Volumes/mayrain/workspace/private/grok2api/app/main.py`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/_account_selection.py`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/snapshot_mapping.rs`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/request_url/mod.rs`
* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/dispatch/pool_scheduler.rs`
* `/Volumes/mayrain/workspace/Aether/apps/aether-gateway/src/handlers/admin/provider/pool_admin/mod.rs`
