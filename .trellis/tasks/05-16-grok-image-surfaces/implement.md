# Implementation Plan

## Checklist

1. Extend provider-transport Grok request-body construction so image intent is
   recognized for `openai:image`, `openai:responses`, and image-model
   `openai:chat` requests.
2. Add Grok runtime helpers that decide when collected images should become
   structured image output and build `image_generation_call` items.
3. Keep OpenAI Images responses using the existing `openai_image_body`.
4. Extend Aether format canonical parsing so OpenAI Responses
   `image_generation_call` can convert to Chat/image-compatible content.
5. Add focused unit tests in provider-transport, aether-ai-formats, and gateway
   Grok runtime tests.
6. Run targeted checks before broader cargo check.

## Validation Commands

```bash
rtk cargo test -p aether-provider-transport grok --lib
rtk cargo test -p aether-ai-formats image_generation_call --lib
rtk cargo test -p aether-gateway grok --lib
rtk cargo check -p aether-gateway
rtk cargo fmt --check
```

## Risk Points

* Do not change public route classification in this task.
* Do not remove existing `openai:image` behavior.
* Do not make generic executors inspect provider names.
* Do not introduce a second Grok-only response conversion path unless the
  generic format matrix cannot represent the shape.

## Review Fixes

* Preserve `image_generation_call` through OpenAI Responses sync-to-stream
  bridging by adding a canonical stream event and Responses client emitter
  output-item support.
* Treat `image_generation_call.result` as base64-only. URL-backed generated
  images use `url`, and Images API bridging emits `data[].url` instead of
  invalid `data[].b64_json = "https://..."`.
* Preserve Grok Responses text exactly when emitted; use trimming only for
  emptiness checks.
* Match grok2api's stable behavior at the capability level, not by copying
  endpoint handlers: Grok image-model `openai:chat` / `openai:responses`
  streams collect generated images first, then pass one canonical response
  through Aether's existing standard bridge.
* Only explicit image-generation intent emits structured
  `image_generation_call` output. Non-image Grok responses that happen to carry
  image URLs keep the existing text-compatible output.
* Custom client model aliases do not get a Grok-specific alias system. Grok
  image intent also checks Aether's existing `report_context.mapped_model`, so
  an alias mapped to `grok-imagine-image*` still gets structured image output.
* Keep OpenAI Images public validation aligned with provider capability:
  requests for Grok image models may use `n=1..4`; other or missing models fail
  early at `n=1` instead of entering candidate execution and exhausting later.
* OpenAI Responses provider streams must not treat `response.output_item.added`
  with `status: generating` as the final image item. Wait for
  `response.output_item.done` or `response.completed` before emitting the
  canonical `image_generation_call`, so later completed metadata/result values
  are not discarded by duplicate suppression.
* Keep Grok app-chat generation count limits consistent across public Images,
  Chat/Responses body building, and Imagine WS runtime. The current Aether
  ceiling is `n=1..4`.
* Model Mapping tests should pass the resolved source model/capabilities into
  default request body generation, matching the Models tab behavior for
  image-capable Responses endpoints.
