# Unify Grok image generation across Aether surfaces

## Goal

Let Grok image models work through Aether's existing public AI surfaces:
`/v1/images/generations`, `/v1/responses`, and `/v1/chat/completions`.
The implementation must reuse Aether's route classification, provider
transport, runtime execution, and format conversion layers instead of copying
`grok2api` endpoint handlers into the gateway.

## User Value

Clients can call Grok image models through familiar OpenAI-compatible entry
points while operators keep one provider runtime and one protocol conversion
model in Aether.

## Confirmed Facts

* Aether already classifies `/v1/chat/completions`, `/v1/responses`,
  `/v1/responses/compact`, `/v1/images/generations`, and `/v1/images/edits`.
* Aether does not classify `/v1/images/variations` or singular
  `/v1/response`.
* Grok runtime already collects generated image URLs from app-chat and Imagine
  WebSocket responses.
* Current Grok `openai:image` responses are structured as image results, but
  Grok `openai:responses` / `openai:chat` paths can collapse image URLs into
  text or content placeholders.
* `grok2api` is useful as behavior reference: image models are detected by
  capability and routed to lite chat or Imagine WebSocket internally. Its
  public route handlers should not be copied into Aether.
* Aether's `image_bridge`, `sync_to_stream`, and standard format matrix already
  contain OpenAI Responses image-generation primitives.

## Requirements

* Do not add new public routes for Grok.
* Do not duplicate `grok2api` endpoint handlers.
* Keep Grok-specific model capability decisions inside Grok provider transport
  or Grok runtime helpers.
* Keep client protocol conversion inside `aether-ai-formats` where possible.
* Make `/v1/responses` return structured `image_generation_call` output for
  Grok image generation.
* Preserve `/v1/images/generations` behavior and reuse the same collected image
  data.
* Keep `/v1/chat/completions` compatible by converting structured image output
  through existing Chat conversion rules rather than adding a Grok-only chat
  response shape.
* Keep image edit and image variation out of this first implementation unless
  they fall out naturally from existing `/v1/images/edits` behavior.

## Acceptance Criteria

* A Grok `openai:responses` request with an `image_generation` tool or a Grok
  image model produces a Responses output item with
  `type = "image_generation_call"` when images are collected.
* A Grok `openai:image` request still returns an OpenAI Images `data[]` body.
* A Grok `openai:chat` request for an image model remains a valid Chat
  Completions response and obtains image content through Aether's generic
  conversion path.
* Provider-transport builds Grok app-chat image payload fields from all
  supported Aether client surfaces instead of only `openai:image`.
* Tests cover request-body construction, Responses output shape, and
  Responses-to-Chat conversion for image generation calls.

## Out Of Scope

* Adding `/v1/images/variations`.
* Adding singular `/v1/response`.
* Replacing the existing OpenAI image endpoint.
* Live Grok network integration tests.
* A new Grok-specific frontend client protocol.
