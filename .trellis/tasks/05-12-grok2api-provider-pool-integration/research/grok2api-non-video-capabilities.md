# Grok2API Non-Video Capability Research

## Scope

Reference project: `/Volumes/mayrain/workspace/private/grok2api`

This document covers the Grok account-to-API behaviors relevant to native Aether integration. Video endpoints, video model registration, video protocol files, and Aether-owned image upload/cache management are explicitly out of scope.

## Public Non-Video API Surface

`grok2api` exposes the non-video endpoints relevant to the Aether proxy surface:

* `GET /v1/models`
* `GET /v1/models/{model_id}`
* `POST /v1/chat/completions`
* `POST /v1/responses`
* `POST /v1/messages`
* `POST /v1/images/generations`
* `POST /v1/images/edits` as protocol reference only when it depends on asset upload/cache
* `GET /v1/files/image` as `grok2api` reference behavior only, not an Aether target in this phase

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:64`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:213`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:433`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:526`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:592`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/anthropic/router.py:79`

## Dynamic Model Visibility

`/v1/models` is not a static registry dump. A model is visible only when:

* its `ModelSpec` is enabled,
* its capability matches the endpoint,
* at least one compatible account pool is currently manageable.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/control/model/registry.py:12`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/model/spec.py:50`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/model/spec.py:63`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:64`

Model families found in `grok2api`:

* Chat: `grok-4.20-*`, `grok-4.3-beta`
* Image generation: `grok-imagine-image-lite`, `grok-imagine-image`, `grok-imagine-image-pro`
* Image edit: `grok-imagine-image-edit`
* Video: `grok-imagine-video`, excluded from Aether scope for this task

## Chat Completions

`/v1/chat/completions` is the main dispatcher. It validates OpenAI-style messages, then routes by model capability:

* image edit models route to image edit,
* image models route to image generation,
* chat models route to the Grok app-chat protocol.

The chat implementation flattens multimodal messages, injects tools, reserves an account, streams Grok SSE, and emits OpenAI-compatible chunks.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:148`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:213`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/chat.py:301`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/chat.py:449`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/protocol/xai_chat.py:16`

## Responses API

`/v1/responses` is a supported subset, not full OpenAI Responses parity. It normalizes `input`/`instructions` into internal messages, uses the same Grok chat path, and reconstructs response events for streaming.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/schemas.py:62`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/responses.py:127`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/responses.py:209`

Aether design implication:

* Start with a documented subset and fail loud for unsupported Responses fields where Aether normally enforces strict behavior.

## Anthropic Messages

`/v1/messages` is a compatibility layer over the same reverse Grok pipeline. It converts Anthropic messages and system content into internal Grok messages, then returns Anthropic-shaped output.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/products/anthropic/router.py:79`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/anthropic/messages.py:64`

Aether design implication:

* This should map to a `claude:messages` endpoint only if Aether already has safe conversion and output shaping for the selected Grok model family.

## Image Generation

There are two image-generation paths:

* `grok-imagine-image-lite` uses the chat endpoint and fast quota.
* `grok-imagine-image` and `grok-imagine-image-pro` use the Grok Imagine WebSocket endpoint.

The WebSocket path supports progress events, multiple images, final image URLs or blobs, and account feedback on auth/forbidden failures.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py:266`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py:272`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/runtime/endpoint_table.py:38`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/transport/imagine_ws.py:278`

Aether design implication:

* Native Grok image generation probably needs a new Grok media transport path, not only the existing OpenAI chat transport.
* The Lite model can be a smaller first step because it rides the chat endpoint.

## Image Edit

Image edit accepts text plus one or more image inputs, uploads references to Grok assets, rewrites placeholders, and collects final asset URLs from streamed Grok responses.

Aether design implication:

* Treat image edit as reference-only when it requires Aether-owned asset upload/cache. The current target is downstream clients calling Aether as transit for supported endpoints, not Aether persisting image assets.

Constraints found:

* `mask` is rejected.
* Only `1024x1024` size is accepted.
* `n` is capped at 2.
* Input images may require upload and asset-reference resolution.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py:526`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py:620`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py:639`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py:691`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/protocol/xai_image_edit.py:13`

## File Upload and Image Cache Reference

`grok2api` image edit and result formatting depend on asset upload/download and local image cache behavior:

* upload uses `/rest/app-chat/upload-file`,
* remote/local image references are normalized to Grok asset references,
* output may be returned as URL, local URL, markdown, or base64 depending on config.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/runtime/endpoint_table.py:17`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/transport/asset_upload.py:87`
* `/Volumes/mayrain/workspace/private/grok2api/app/platform/storage/media_paths.py:16`
* `/Volumes/mayrain/workspace/private/grok2api/app/platform/storage/media_cache.py:31`

Security note:

* Aether should not copy remote URL fetch or local cache behavior. Remote image URL fetching can become SSRF or cookie leakage if account cookies are attached to attacker-controlled URLs.
* This section is reference-only for understanding `grok2api`; it is not part of the Aether implementation scope.

## Account Selection, Refresh, and Feedback

`grok2api` has its own pool system, but Aether should use it only as a behavior reference. Important behaviors to reproduce in Aether-native terms:

* pool candidates are derived from model tier and `prefer_best`,
* account selection can be quota-aware or random,
* on-demand refresh can run before declaring no accounts available,
* feedback updates quota, failures, invalid credential state, and cooling.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/products/_account_selection.py:15`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/account/selector.py:61`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/account/refresh.py:133`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/account/feedback.py:32`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/account/invalid_credentials.py:16`

## Upstream Grok/XAI Protocol Endpoints

Non-video endpoints used by `grok2api`:

* Chat: `https://grok.com/rest/app-chat/conversations/new`
* Upload: `https://grok.com/rest/app-chat/upload-file` (`grok2api` reference only)
* Assets: `https://grok.com/rest/assets`, `https://assets.grok.com` (`grok2api` reference only)
* Quota: `https://grok.com/rest/rate-limits`
* Image WebSocket: `wss://grok.com/ws/imagine/listen`
* Auth/TOS/feature control: `accounts.x.ai` and Grok gRPC-Web/REST endpoints

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/runtime/endpoint_table.py:11`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/protocol/xai_usage.py:101`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/protocol/xai_assets.py:15`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/protocol/xai_auth.py:34`

## Auth, Proxy, and Clearance Materials

Grok account access is not a simple API key. Runtime requests construct:

* SSO cookies,
* optional `cf_clearance`,
* browser-like headers and hints,
* proxy leases for HTTP and WebSocket paths.

Key references:

* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/proxy/adapters/headers.py:172`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/proxy/adapters/session.py:36`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/proxy/models.py:20`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/proxy/feedback.py:6`

Aether design implication:

* Grok `auth_config` needs structured account/session metadata and redaction rules.
* Arbitrary proxy URLs should not be imported; use Aether-approved proxy node IDs.

## Out of Scope

* `/v1/videos`
* `/v1/videos/{video_id}`
* `/v1/videos/{video_id}/content`
* `grok-imagine-video`
* `xai_video.py` and related video job state
* Aether-owned image upload/cache persistence
