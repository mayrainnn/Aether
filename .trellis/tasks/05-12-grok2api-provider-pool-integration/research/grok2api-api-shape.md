# grok2api API Shape and Pool Semantics

## Summary

`grok2api` is a FastAPI-based Grok gateway that exposes OpenAI-compatible and Anthropic-compatible endpoints, plus admin/web UI surfaces. Its account management is not a single static credential path; the service already treats accounts as a pool with revisioned storage, sync loops, refresh scheduling, quota windows, and invalid-credential handling.

## Public API Surface

The documented public API includes:

- `GET /v1/models`
- `GET /v1/models/{model_id}`
- `POST /v1/chat/completions`
- `POST /v1/responses`
- `POST /v1/messages`
- `POST /v1/images/generations`
- `POST /v1/images/edits`
- `POST /v1/videos`
- `GET /v1/videos/{video_id}`
- `GET /v1/videos/{video_id}/content`

Source: [`README.md`](/Volumes/mayrain/workspace/private/grok2api/README.md#L269-L284)

The application mounts the OpenAI router and Anthropic router in the FastAPI app factory.

Source: [`app/main.py`](/Volumes/mayrain/workspace/private/grok2api/app/main.py#L276-L405)

The OpenAI router handles `/v1/models`, `/v1/chat/completions`, and `/v1/responses` with OpenAI-style request validation and SSE streaming.

Source: [`app/products/openai/router.py`](/Volumes/mayrain/workspace/private/grok2api/app/products/openai/router.py#L64-L260)

The Anthropic router handles `/v1/messages` and converts it into the same internal message pipeline.

Source: [`app/products/anthropic/router.py`](/Volumes/mayrain/workspace/private/grok2api/app/products/anthropic/router.py#L16-L125)

## Authentication Model

- `/v1/*` uses `app.api_key` when configured.
- `/admin/*` uses `app.app_key`, defaulting to `grok2api`.
- `/webui/*` uses `app.webui_enabled` and `app.webui_key`.
- `/v1/*` accepts either `Authorization: Bearer <key>` or `X-API-Key: <key>`.
- If `app.api_key` is empty, the `/v1/*` routes do not add an extra auth check.

Sources:
- [`README.md`](/Volumes/mayrain/workspace/private/grok2api/README.md#L144-L150)
- [`app/platform/auth/middleware.py`](/Volumes/mayrain/workspace/private/grok2api/app/platform/auth/middleware.py#L17-L118)

## Models and Modalities

The service exposes Grok chat, image, and video model families through `GET /v1/models` and the docs enumerate:

- chat models such as `grok-4.20-auto`, `grok-4.20-fast`, `grok-4.20-expert`, `grok-4.20-heavy`
- image models such as `grok-imagine-image-lite`, `grok-imagine-image`, `grok-imagine-image-pro`, `grok-imagine-image-edit`
- video model `grok-imagine-video`

Sources:
- [`README.md`](/Volumes/mayrain/workspace/private/grok2api/README.md#L224-L265)
- [`README.md`](/Volumes/mayrain/workspace/private/grok2api/README.md#L273-L284)
- [`README.md`](/Volumes/mayrain/workspace/private/grok2api/README.md#L316-L597)

## Account Pool Semantics

The account control plane is pool-native:

- storage backends: `local` SQLite, `redis`, `mysql`, `postgresql`
- account repository contract supports initialize, revisioning, snapshot sync, change scan, CRUD, list, and atomic pool replace
- account records carry `pool`, quota windows, usage counters, lifecycle status, and extension data
- default quota sets differ by pool size (`basic`, `super`, `heavy`)
- scheduler selection has two modes:
  - `quota` strategy scores by health, quota, inflight, recent use, and failures
  - `random` strategy ignores quota/health and picks among non-cooling candidates
- the leader worker runs the heavy refresh scheduler; all workers run a lightweight directory sync loop
- success/failure feedback mutates quota windows and account state
- invalid credentials are detected centrally and expire the account

Sources:
- [`app/main.py`](/Volumes/mayrain/workspace/private/grok2api/app/main.py#L115-L210)
- [`app/control/account/repository.py`](/Volumes/mayrain/workspace/private/grok2api/app/control/account/repository.py#L15-L84)
- [`app/control/account/models.py`](/Volumes/mayrain/workspace/private/grok2api/app/control/account/models.py#L17-L260)
- [`app/control/account/quota_defaults.py`](/Volumes/mayrain/workspace/private/grok2api/app/control/account/quota_defaults.py#L43-L172)
- [`app/dataplane/account/selector.py`](/Volumes/mayrain/workspace/private/grok2api/app/dataplane/account/selector.py#L1-L181)
- [`app/control/account/refresh.py`](/Volumes/mayrain/workspace/private/grok2api/app/control/account/refresh.py#L57-L260)
- [`app/control/account/state_machine.py`](/Volumes/mayrain/workspace/private/grok2api/app/control/account/state_machine.py#L101-L260)
- [`app/control/account/invalid_credentials.py`](/Volumes/mayrain/workspace/private/grok2api/app/control/account/invalid_credentials.py#L16-L79)
- [`app/products/web/admin/tokens.py`](/Volumes/mayrain/workspace/private/grok2api/app/products/web/admin/tokens.py#L142-L453)

## Research Takeaway

The upstream project already has the exact class of behavior Aether needs to preserve if it wants pool-level control: per-account lifecycle, quota windows, refresh scheduling, and admin CRUD. That makes `grok2api` a useful behavioral reference, but not a reason to keep a single static secret in Aether.
