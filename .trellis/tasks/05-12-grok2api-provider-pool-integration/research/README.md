# Grok Account-to-API Research Index

## Decision

Use `grok2api` as the Grok protocol reference, but implement the target as an Aether-native Grok provider family.

Video is excluded. The researched target surface is:

* `/v1/models`
* `/v1/chat/completions`
* `/v1/responses`
* `/v1/messages`
* `/v1/images/generations`
* `/v1/images/edits` as protocol reference only; Aether is not taking ownership of the upload/cache path in this phase
* Aether provider-key based account import, selection, health, quota, and feedback

## Read Order

1. `grok-native-provider-design.md`
   * Native Aether design, phased scope, and verification plan.
2. `aether-native-provider-patterns.md`
   * Codex, Kiro, and Antigravity implementation patterns to reuse.
3. `grok2api-non-video-capabilities.md`
   * Non-video endpoint and image capability map from `/Volumes/mayrain/workspace/private/grok2api`.
4. `aether-pool-architecture.md`
   * Aether provider catalog, transport snapshot, scheduler, and admin pool architecture.
5. `integration-options.md`
   * Option comparison with native Aether provider family as the recommended path.
6. `security-ops.md`
   * Required guardrails around session secrets, proxy material, remote media, and admin exposure.
7. `implementation-notes.md`
   * Native implementation scope, verification evidence, and remaining risk notes.

## Supplemental Notes

* `grok2api-api-shape.md` and `grok2api-capabilities.md` are earlier broad scans kept for traceability.
* The current plan should not copy `grok2api` account storage, admin credentials, or media-fetch/cache behavior directly.
* Before implementation, load Trellis development specs and run GitNexus impact analysis for any symbols that will be edited.
* Implementation now exists on `feat/grok2api-provider-pool-integration`; video and Aether-owned image upload/cache remain excluded.
