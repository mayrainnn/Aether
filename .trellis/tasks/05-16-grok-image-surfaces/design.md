# Design

## Architecture Boundary

Grok image support should align with Aether's existing layered model:

* Route classification stays in `apps/aether-gateway/src/control`.
* Provider request construction stays in `crates/aether-provider-transport`.
* Grok account execution and asset collection stay in
  `apps/aether-gateway/src/execution_runtime/grok.rs`.
* Cross-surface response conversion stays in `crates/aether-ai-formats`.

`grok2api` is a behavior reference only. It shows that image models can be
detected by model capability and sent through either lite chat or Imagine
WebSocket. Aether should implement that decision inside its existing Grok
provider/runtime layers, not by adding duplicate FastAPI-style route handlers.

## Data Flow

1. A client request enters an existing Aether route:
   `/v1/images/generations`, `/v1/responses`, or `/v1/chat/completions`.
2. Aether planner selects a Grok candidate and builds an `ExecutionPlan`.
3. Provider transport builds a Grok app-chat body from the original client
   surface body. If the request is image intent, it sets Grok image fields such
   as `imageGenerationCount`.
4. Grok runtime executes the app-chat or Imagine WebSocket path and collects
   text, reasoning, and image URLs/data URLs.
5. Grok runtime builds one provider-side canonical OpenAI Responses body. When
   images exist and the request is image intent, images become
   `image_generation_call` output items.
6. Aether's existing format conversion bridges that provider-side Responses
   body to the client surface:
   * `/v1/responses`: keep Responses shape.
   * `/v1/images/generations`: bridge to Images `data[]`.
   * `/v1/chat/completions`: convert image output through generic Chat
     conversion.

## Compatibility

The stable `/v1/images/generations` behavior should remain unchanged. Chat
Completions should stay a valid chat response; clients needing structured image
metadata should prefer `/v1/responses` or `/v1/images/generations`.

## Risk Controls

The Grok runtime entry points have high blast radius because all local execution
flows pass through them. Keep the patch local to image intent predicates and
response builders. Do not change route classification, candidate loops, auth,
or transport backend selection.
