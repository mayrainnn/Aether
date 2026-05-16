# aether-ai-formats Backend Guidelines

This spec covers `crates/aether-ai-formats`, the Rust foundation crate that owns AI API
format identities, canonical request/response structures, request/response conversion,
stream rewriting, provider-private envelope handling, and execution-runtime control
payload formats.

The crate is intentionally pure. It does not open sockets, query databases, use Redis,
own axum handlers, or emit logs. Callers pass `serde_json::Value`, request metadata, and
report context into this crate; this crate returns typed canonical values, JSON payloads,
byte buffers, or small typed errors for the gateway and service crates to handle.

## Pre-Development Checklist

- Read [Directory Structure](./directory-structure.md) before adding modules or exports.
- Read [Error Handling](./error-handling.md) before changing parsing, emitting, SSE, or
  provider-private bridge code.
- Read [Quality Guidelines](./quality-guidelines.md) before adding a format, alias, or
  conversion matrix entry.
- Read [Logging Guidelines](./logging-guidelines.md) before adding observability behavior.
- Read [Streaming And Provider Compatibility](./streaming-and-provider-compat.md) before
  touching `formats/shared/stream_*`, provider-private envelopes, Kiro stream conversion,
  or local proxy rule handling.
- Keep this crate free of database and runtime side effects. The removed
  `database-guidelines.md` template is not applicable to this crate because
  `crates/aether-ai-formats/Cargo.toml:9-19` contains only pure data and parsing
  dependencies, and source search finds no `sqlx`, SeaORM, Redis, transaction, or
  connection APIs under `crates/aether-ai-formats/src`.

## Guidelines Index

| Guide | Scope |
|-------|-------|
| [Directory Structure](./directory-structure.md) | Module layout, public facade rules, provider-specific file placement, and where new format code belongs. |
| [Error Handling](./error-handling.md) | `FormatError`, `AiSurfaceFinalizeError`, provider-shaped error bodies, image request errors, and parse failure rules. |
| [Quality Guidelines](./quality-guidelines.md) | Naming, visibility, canonical data model rules, alias safety, testing expectations, and forbidden patterns. |
| [Logging Guidelines](./logging-guidelines.md) | Current no-logging policy, sanitized metadata rules, and where logging must live if a caller needs it. |
| [Streaming And Provider Compatibility](./streaming-and-provider-compat.md) | Streaming state machines, SSE emission, provider-private envelopes, Kiro event stream decoding, and local proxy rules. |

## Architecture Summary

- `src/lib.rs` is the minimal crate facade. It declares top-level modules and re-exports
  only stable cross-crate APIs such as `FormatContext`, `FormatError`, format identity
  helpers, registry conversions, model directives, and canonical types.
- `src/api.rs` is the broad application-facing compatibility facade. Gateway and service
  crates import from here when they need many Aether AI surface helpers in one place.
- `src/contracts/` owns execution-runtime action strings, plan kind strings, report kind
  strings, and control payload structs.
- `src/formats/` owns provider-specific request, response, stream, image, video,
  embedding, rerank, registry, matrix, and shared helpers.
- `src/protocol/` owns provider-neutral canonical structs and stream frame structs.
- `src/provider_compat/` owns provider-private envelope descriptors, Kiro stream
  conversion, and local proxy header/body rule evaluation.

## Evidence From Code Intelligence

GitNexus repo context for `repo="Aether"` reports 3,140 files, 83,229 symbols, and 300
execution flows. The process resource shows a direct downstream consumer flow where
gateway provider query execution reaches `header_rules_are_locally_supported` in
`crates/aether-ai-formats/src/provider_compat/proxy/rules.rs` through provider-transport
policy. This confirms the crate participates as a foundation decision library, not as a
request handler.

ABCoder AST data for `repo_name="aether-ai-formats"` lists 107 Rust source files and
symbol nodes such as:

```text
aether-ai-formats::formats::registry#convert_request
aether-ai-formats::formats::registry#convert_response
aether-ai-formats::formats::context#FormatError
aether-ai-formats::provider_compat::proxy::rules#apply_local_header_rules
```

Use that shape when adding specs: document stable public nodes, module-local helpers, and
tests that lock behavior.

## Quality Check

- Every new or changed guideline references concrete source paths and line numbers.
- No placeholder text or HTML comments remain in this spec directory.
- `database-guidelines.md` stays deleted unless this crate gains actual database code.
- New format work updates all relevant files: identity, matrix, provider module,
  canonical conversion, registry, stream handling if applicable, API facade, and tests.
- Verification should include at least `cargo test -p aether-ai-formats` for any source
  change in this crate. Documentation-only changes can still run this command when the
  workspace is available to catch stale examples and path drift.
