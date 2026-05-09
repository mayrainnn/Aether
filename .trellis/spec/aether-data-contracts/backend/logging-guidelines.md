# Logging Guidelines

> Logging and observability rules for the `aether-data-contracts` crate.

---

## Overview

`aether-data-contracts` currently performs no logging. That is intentional.
This crate defines data contracts and validation helpers; it does not execute
SQL, call Redis, run background work, serve HTTP requests, or own retry loops.

The dependency list at `crates/aether-data-contracts/Cargo.toml:9` does not
include `tracing`, `log`, `tokio`, `axum`, `sqlx`, `sea-orm`, or `redis`.
Search evidence in the crate shows no `tracing::`, `debug!`, `info!`, `warn!`,
or `error!` macro usage. The only "tracing" wording is domain terminology such
as `RequestCandidateTrace`, not observability instrumentation.

Therefore, this spec is a no-logging contract: keep observability decisions in
callers and concrete repositories, and keep this crate deterministic and side
effect free.

---

## What This Crate Should Do Instead Of Logging

Return typed errors with field-qualified messages. Example:

```rust
pub fn from_database(value: &str) -> Result<Self, crate::DataLayerError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "queued" => Ok(Self::Queued),
        other => Err(crate::DataLayerError::UnexpectedValue(format!(
            "unsupported background_tasks.status: {other}"
        ))),
    }
}
```

Source:
`crates/aether-data-contracts/src/repository/background_tasks/types.rs:65`.

Return typed summaries instead of logging progress. `UsageCleanupSummary` is a
contract result with counts for `body_externalized`, `legacy_body_refs_migrated`,
`body_cleaned`, `header_cleaned`, `keys_cleaned`, and `records_deleted` at
`crates/aether-data-contracts/src/repository/usage/types.rs:1707`.

Expose structured state to callers. `UsageBodyCaptureResult::as_json_entry`
builds a JSON map containing `available`, `storage`, `state`, and optional
`body_ref` at `crates/aether-data-contracts/src/repository/usage/types.rs:1218`;
callers can decide whether to log that redacted summary.

---

## Log Levels

This crate should not choose log levels. It has no runtime event boundary where
`debug`, `info`, `warn`, or `error` would be meaningful.

Higher layers may map contract outcomes to levels. Suggested caller mapping:

- `InvalidInput`: usually `debug` or request-scoped validation feedback.
- `UnexpectedValue`: usually `warn` or `error`, depending on whether stored data
  corruption affects user-visible behavior.
- `Postgres`, `Redis`, `Sql`, and `TimedOut`: usually `warn` or `error` in the
  concrete repository or service boundary.

Do not encode this mapping in `aether-data-contracts`. The enum only provides
the vocabulary at `crates/aether-data-contracts/src/error.rs:2`.

---

## Structured Fields

When adding contract result types that callers may log, expose structured
fields instead of preformatted log strings.

Good existing examples:

- `BackgroundTaskSummary` exposes `total`, `running_count`, `by_status`, and
  `by_kind` at
  `crates/aether-data-contracts/src/repository/background_tasks/types.rs:233`.
- `StoredProviderCatalogKeyStats` exposes `provider_id`, `total_keys`, and
  `active_keys` at
  `crates/aether-data-contracts/src/repository/provider_catalog/types.rs:537`.
- `VideoTaskStatusCount` exposes typed `status` plus `count` at
  `crates/aether-data-contracts/src/repository/video_tasks/types.rs:360`.

Avoid returning one large string such as `"provider p1 has 3 active keys"`.
That makes caller-side redaction, metrics, and localization harder.

---

## Sensitive Data Rules

Because this crate defines contracts for request/response bodies, headers,
provider API keys, OAuth credentials, usage audits, and billing metadata, any
future logging would be high risk. Do not add logs that include these fields:

- `StoredProviderCatalogKey.encrypted_api_key` and
  `encrypted_auth_config` at
  `crates/aether-data-contracts/src/repository/provider_catalog/types.rs:253`.
- Usage request/response headers and bodies at
  `crates/aether-data-contracts/src/repository/usage/types.rs:61`.
- `UpsertUsageRecord.request_headers`, `request_body`, `provider_request_body`,
  `response_body`, and `client_response_body` at
  `crates/aether-data-contracts/src/repository/usage/types.rs:1568`.
- Provider key health/status JSON fields such as `fingerprint`,
  `upstream_metadata`, and `status_snapshot` at
  `crates/aether-data-contracts/src/repository/provider_catalog/types.rs:263`.

If a caller needs observability, expose redacted IDs, counts, enum states, and
contract-level summaries. Never add logging inside a constructor or `validate`
method as a substitute for returning `DataLayerError`.

---

## Span And Instrumentation Rules

Do not use `#[tracing::instrument]` or create spans in this crate. Repository
trait methods are interfaces, not execution points. Instrument concrete
implementations in `aether-data`, where the code has access to backend type,
query name, retry count, transaction scope, and latency.

Example interface that should remain uninstrumented:

```rust
#[async_trait]
pub trait UsageWriteRepository: Send + Sync {
    async fn upsert(
        &self,
        usage: UpsertUsageRecord,
    ) -> Result<StoredRequestUsageAudit, crate::DataLayerError>;
}
```

Source:
`crates/aether-data-contracts/src/repository/usage/types.rs:1664`.

Adding instrumentation here would either do nothing for implementors or force a
tracing dependency into every crate that only wants the contracts.

---

## Common Mistakes

Do not log from validation helpers:

```rust
// DON'T: validation should be deterministic and side-effect free.
if self.request_id.trim().is_empty() {
    tracing::warn!("empty request id");
    return Err(crate::DataLayerError::InvalidInput(...));
}
```

Return the error only. `UsageSettlementInput::validate` demonstrates the
side-effect-free pattern at
`crates/aether-data-contracts/src/repository/settlement/types.rs:18`.

Do not add a logging dependency to support one debugging session. If a contract
needs more context, improve the `DataLayerError` message or add a structured
field to the returned DTO.

Do not log serialized `serde_json::Value` fields from this crate. Many of those
values can contain request payloads, auth metadata, provider configuration, or
billing details. Keep logging redaction close to the caller that understands
the data source.

