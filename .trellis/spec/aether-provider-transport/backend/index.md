# aether-provider-transport Backend Guidelines

`aether-provider-transport` is the domain crate that turns catalog snapshots into
local upstream-provider HTTP requests. It owns provider-specific request shape,
auth header construction, URL selection, streaming policy, OAuth refresh
coordination, and diagnostics. It does not own HTTP route handlers, persistence,
or database queries.

The crate is indexed in GitNexus under repo `Aether`. GitNexus places the crate
in the Provider functional area and in gateway/admin flows such as
`Provider_query_execute_standard_test_candidate -> Same_api_format`, where
`apps/aether-gateway` calls `supports_local_standard_transport_with_network` and
the policy helpers in `crates/aether-provider-transport/src/policy.rs`.

## Pre-Development Checklist

Before changing this crate:

1. Read `crates/aether-provider-transport/src/lib.rs` to understand the public
   facade and re-export boundaries.
2. Read the provider-specific module you are touching, plus the shared helper it
   delegates to: `auth`, `request_url`, `policy`, `same_format_provider`,
   `oauth_refresh`, `snapshot`, or `network`.
3. Use GitNexus with `repo="Aether"` for impact and flow checks before editing
   symbols. For example, the standard-provider admin test flow reaches
   `policy.rs` through `apps/aether-gateway/src/handlers/admin/provider/query/models.rs`.
4. Treat ABCoder results as the preferred symbol-level source when the ABCoder
   MCP server is available with `repo_name="aether-provider-transport"`.
5. Confirm whether the change affects same-format transport, cross-format
   conversion, fixed-provider templates, OAuth refresh, or diagnostics.
6. Add or update focused unit tests in the touched module. This crate keeps most
   tests next to the private helpers they protect.

## Guidelines Index

| Guide | Purpose |
|-------|---------|
| [Directory Structure](./directory-structure.md) | Module layout, public facade rules, provider-specific boundaries |
| [Error Handling](./error-handling.md) | Error enums, `Result` and `Option` semantics, unsupported-reason patterns |
| [Quality Guidelines](./quality-guidelines.md) | Type safety, visibility, normalization, testing, and anti-patterns |
| [Logging Guidelines](./logging-guidelines.md) | `tracing` usage, structured fields, log levels, and sensitive-data rules |
| [Streaming and Request Building](./streaming-and-request-building.md) | URL, header, body, SSE, custom-path, and provider behavior conventions |

## Non-Applicable Template

`database-guidelines.md` is intentionally absent. This crate has no SeaORM,
Redis, sqlx, migration, transaction, or connection handling. The closest
persistence boundary is `ProviderTransportSnapshotSource`, an async trait that
receives already-abstracted repository records and returns snapshot data through
`DataLayerError` (`crates/aether-provider-transport/src/snapshot.rs:77`).

Do not add database access here. Keep database/repository concerns in the data
layer and pass transport snapshots into this crate.

## Quality Check

Run the checks that prove the edited surface:

1. Template scan: search this directory for generated boilerplate, empty
   sections, and comment blocks before finishing.
2. Documentation shape:
   `wc -l .trellis/spec/aether-provider-transport/backend/*.md`
3. Rust regression tests when code changes accompany spec work:
   `cargo test -p aether-provider-transport`
4. GitNexus change-scope check before committing code changes:
   `gitnexus_detect_changes({scope: "all", repo: "Aether"})`

## Review Focus

Reviewers should verify:

1. Public exports are deliberate and match `src/lib.rs`.
2. Unsupported-provider decisions return stable reason strings instead of
   panicking or logging-only failures.
3. URL/header/body builders preserve auth and content invariants.
4. Streaming behavior is driven by endpoint config plus request requirements,
   not by ad hoc caller booleans.
5. Token values, API keys, and raw proxy URLs are not logged or added to
   diagnostics.
