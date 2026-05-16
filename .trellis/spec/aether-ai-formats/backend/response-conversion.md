# Response Conversion

`aether-ai-formats` owns cross-surface response shape preservation. When a
provider returns an OpenAI Responses `image_generation_call`, parse it into a
canonical image block but keep enough OpenAI Responses extension metadata to
rebuild the same item type later. Do not silently downgrade generated images to
plain text URLs or generic `output_image` items when the source shape was an
image generation call.

For Chat Completions compatibility, canonical image blocks may emit
`image_url` content parts. Structured image metadata should remain available on
Responses and Images surfaces; Chat is the compatibility view, not the canonical
image generation contract.

Streaming conversion has the same contract as sync conversion. OpenAI Responses
provider streams that complete with `image_generation_call` items must emit a
canonical image-generation stream event so the OpenAI Responses client emitter
can reproduce `response.output_item.done` and `response.completed.output[]`
without degrading the image to an `output_text` placeholder. Chat, Claude, and
Gemini stream emitters may render the same item as a generic image content part
for compatibility.

Treat `response.output_item.added` for `image_generation_call` as provisional
unless the item is already explicitly completed. Providers may include a
temporary `result` while `status` is still `generating`; the canonical
image-generation event should be emitted from `response.output_item.done` or
`response.completed` so the final status, metadata, and image payload win.

`image_generation_call.result` is base64 image data, not a generic image
location field. If an upstream image is represented by an HTTP(S) URL, keep it
in `url` and leave `result` absent. Bridges to OpenAI Images must map base64
payloads to `data[].b64_json` and HTTP(S) assets to `data[].url`; they must not
put a URL string into `b64_json`.
