# Aether Billing Backend Guidelines

Guidelines for `crates/aether-billing`, the shared billing-domain core for the
Aether Rust gateway. Read these before changing billing formulas, pricing
snapshots, usage-event enrichment, or token normalization.

## Pre-Development Checklist

- Confirm the change belongs in `aether-billing`: pure billing calculation,
  pricing snapshot interpretation, formula evaluation, or usage-event metadata
  enrichment.
- Keep persistence, request routing, SeaORM entities, Redis access, and runtime
  workers outside this crate.
- Preserve `lib.rs` as the public API surface and keep implementation modules
  private unless a stable re-export is needed.
- Decide whether the change affects stored billing snapshot schema version
  `2.0` or settlement snapshot schema version `3.0`.
- Add or update module-local tests, then run `cargo test -p aether-billing`.

## Guidelines Index

| Guide | Description |
|-------|-------------|
| [Directory Structure](./directory-structure.md) | Crate layout, public boundary, module responsibilities, and dependency limits |
| [Error Handling](./error-handling.md) | Formula errors, incomplete/no-rule states, and enrichment-layer error conversion |
| [Quality Guidelines](./quality-guidelines.md) | Money precision, snapshot stability, visibility, token normalization, and tests |
| [Logging Guidelines](./logging-guidelines.md) | Current no-logging baseline and where caller-owned tracing belongs |

## Removed Template

`database-guidelines.md` was intentionally removed. This crate does not use
SeaORM, Redis, migrations, database connections, or transactions. The only
database-shaped operation is the caller-provided lookup trait:

```rust
// crates/aether-billing/src/event_enrichment.rs:15
pub trait BillingModelContextLookup: Send + Sync {
    async fn find_billing_model_context_by_model_id(
        &self,
        provider_id: &str,
        provider_api_key_id: Option<&str>,
        model_id: &str,
    ) -> Result<Option<StoredBillingModelContext>, DataLayerError> {
```

Implement that trait in a data/application crate. Do not add direct storage
access to `aether-billing`.

## Quality Gate

Before reporting work complete:

- A placeholder/comment-marker search over this directory returns no matches.
- Each retained spec file contains real examples with source file paths.
- `find .trellis/spec/aether-billing/backend -maxdepth 1 -type f | sort`
  matches the four guide files plus this `index.md`.
- For code changes, run `cargo test -p aether-billing`.

## Source Evidence Used

These guidelines were derived from:

- GitNexus repo context for `Aether`, including repo stats and module clusters.
- ABCoder AST parse with `repo-id=aether-billing`, including graph edges for
  `BillingService.calculate`, `FormulaEngine.evaluate`, and
  `enrich_usage_event_with_billing`.
- Direct source review of all files under `crates/aether-billing/src/`.

**Language**: English.
