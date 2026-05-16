# brainstorm: add Grok provider-query model test adapter

## Goal

Add a dedicated Grok adapter for Rust local provider-query model tests so Grok accounts can test chat-capable models like `grok-4.20-0309-non-reasoning` through Aether native Grok runtime behavior.

Follow-up scope added during implementation: model Grok's browser-fingerprint upstream transport as an Aether execution runtime capability using the in-process Rust `wreq` backend. Grok and future browser-sensitive providers should not depend on a Python HTTP sidecar.

Second follow-up scope: support Grok multimodal chat inputs by resolving
OpenAI/Claude-style image and file blocks in memory and uploading them directly
to Grok. Aether must not persist these input assets to local disk, object
storage, DB tables, or durable cache.

## What I already know

* The user hit `Rust local provider-query model test does not support endpoint format openai:chat (...)` while testing `grok-4.20-0309-non-reasoning`.
* Existing Grok provider implementation already has fixed provider templates, account import/quota support, Grok-specific headers, app-chat body conversion, and execution runtime adapter.
* Provider Query model tests use their own adapter selection path with `Standard`, `Kiro`, `OpenAiImage`, and `Antigravity`; there is no Grok adapter yet.
* Grok should not be marked as standard/local transport supported because Grok requires cookie/browser headers and app-chat conversion rather than standard OpenAI-compatible transport semantics.
* Rust-side browser impersonation should be implemented as an execution transport backend, not as Grok-only request code, so other provider runtimes can opt into it later.
* grok2api handles multimodal chat by extracting file/image blocks, uploading
  their base64 content to Grok `/rest/app-chat/upload-file`, and passing the
  returned `fileMetadataId` values through app-chat `fileAttachments`.

## Assumptions

* The initial model-test target is non-video Grok chat behavior, especially `openai:chat` and `grok-4.20-0309-non-reasoning`.
* The adapter should reuse existing Grok transport/runtime helpers instead of duplicating protocol logic.
* It is acceptable for Grok model tests to use the same execution runtime sync path as other model tests.

## Requirements

* Add a dedicated `ProviderQueryTestAdapter::Grok` selected for provider type `grok`.
* Do not enable `supports_local_openai_chat_transport` or `supports_local_same_format_transport` for Grok in runtime policy.
* Build Grok model-test requests with the existing Grok helper functions: `build_grok_upstream_url`, `build_grok_browser_headers`, and `build_grok_app_chat_body` via runtime execution.
* Preserve existing model-test candidate tracing, skip/fail/success response shape, latency/status capture, and request/response diagnostic payloads.
* Add focused tests covering adapter selection and Grok model-test behavior without making a real upstream call.
* Add a Rust in-process browser impersonation transport backend using `wreq`.
* Preserve proxy, timeout, redirect, header, body, sync, and stream behavior through the Rust browser transport backend.
* Extract OpenAI Chat, OpenAI Responses, and Claude Messages attachment inputs
  from the original client body.
* Resolve `data:` URIs and `http` / `https` URLs in memory only, with explicit
  byte limits.
* Upload resolved attachments to Grok `/rest/app-chat/upload-file` with Grok
  browser headers and put returned IDs in app-chat `fileAttachments`.
* Do not write Grok input attachments to Aether storage.

## Acceptance Criteria

* [ ] `grok` provider-query model tests no longer skip `openai:chat` as unsupported solely because standard/local transport is disabled.
* [ ] Grok model-test execution uses Grok-specific request URL, headers, and runtime marker.
* [ ] Existing Standard/Kiro/OpenAiImage/Antigravity model-test behavior is unchanged.
* [ ] Grok provider transport profiles default to `browser_wreq`.
* [ ] Rust execution runtime can execute sync and streaming plans through the in-process browser impersonation backend.
* [ ] Grok chat requests with image/file input blocks upload attachments to
  Grok and do not leak raw URLs/data URIs into the prompt text.
* [ ] Targeted Rust tests pass.
* [ ] `cargo check -p aether-gateway` passes.

## Definition of Done

* Implementation compiles.
* Targeted model-test tests added/updated.
* Existing services can be restarted after the change if needed.

## Out of Scope

* Enabling generic standard/local transport for Grok.
* Real live Grok network integration tests.
* Changing Grok quota/account import behavior.
* Adding video model test support.
* Persisting Grok input attachments in Aether.

## Technical Notes

* Main files: `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test.rs`, `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test/adapter.rs`.
* Reuse helpers from `crates/aether-provider-transport/src/grok.rs` through `crate::provider_transport`.
* Existing runtime adapter lives at `apps/aether-gateway/src/execution_runtime/grok.rs`.
* Browser transport capability files: `crates/aether-contracts/src/plan.rs`, `crates/aether-provider-transport/src/network.rs`, and `apps/aether-gateway/src/execution_runtime/transport.rs`.
