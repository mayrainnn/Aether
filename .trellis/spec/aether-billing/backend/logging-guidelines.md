# Logging Guidelines

`aether-billing` currently does not emit logs or tracing spans. That is
intentional for the pure calculation modules: billing math should be
deterministic, testable, and usable from batch jobs, request paths, and async
event processors without producing duplicate logs.

## Current State

There are no `tracing`, `log`, `println!`, or `eprintln!` calls in
`crates/aether-billing/src/`. `Cargo.toml` also does not depend on `tracing`.

```toml
# crates/aether-billing/Cargo.toml:9
[dependencies]
aether-data-contracts.workspace = true
aether-usage-runtime.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tokio.workspace = true
```

Preserve this no-logging baseline for pure functions such as
`FormulaEngine::evaluate`, `BillingService::calculate`, precision helpers, and
token normalization.

```rust
// crates/aether-billing/src/service.rs:27
pub fn calculate(
    &self,
    pricing: &BillingModelPricingSnapshot,
    input: &BillingUsageInput,
) -> Result<BillingComputation, ExpressionEvaluationError> {
```

## Prefer Structured Results Over Logs

When the calculation is incomplete, return a structured status and the missing
dimension names instead of logging a warning. This gives callers enough context
to decide whether to persist, retry, alert, or suppress.

```rust
// crates/aether-billing/src/formula_engine.rs:152
return Ok(FormulaEvaluationResult {
    status: FormulaEvaluationStatus::Incomplete,
    cost: 0.0,
    resolved_dimensions: dims,
    resolved_variables: resolved,
    cost_breakdown: BTreeMap::new(),
    tier_index,
    tier_info,
    missing_required,
    error: None,
});
```

When no rule exists, return `BillingSnapshotStatus::NoRule` in the snapshot
instead of logging and returning an error.

```rust
// crates/aether-billing/src/service.rs:38
status: BillingSnapshotStatus::NoRule,
snapshot: BillingSnapshot {
    schema_version: BILLING_SNAPSHOT_SCHEMA_VERSION.to_string(),
    rule_id: None,
    rule_name: None,
```

When billing metadata needs to explain what happened, put the evidence into the
event metadata. The enrichment adapter writes `billing_snapshot`,
`settlement_snapshot`, `billing_dimensions`, `rate_multiplier`, and
`is_free_tier`.

```rust
// crates/aether-billing/src/event_enrichment.rs:225
metadata.insert("billing_snapshot".to_string(), billing_snapshot);
metadata.insert(
    "settlement_snapshot_schema_version".to_string(),
    Value::from(SETTLEMENT_SNAPSHOT_SCHEMA_VERSION),
);
metadata.insert("settlement_snapshot".to_string(), settlement_snapshot);
```

## Where Logging Belongs

Caller crates should log request IDs, user IDs, provider API key IDs, and data
layer failures at their own boundary. This crate only receives snapshots and
usage events, then returns a calculation or mutates metadata.

If logging is added later, restrict it to integration boundaries where there is
real operational context:

- a failed `BillingModelContextLookup` call in the caller implementation
- an enrichment failure tied to a request/event ID
- aggregate counters in a runtime service that batches usage events

Do not add logs inside:

- `quantize_value`, `quantize_cost`, or `quantize_display`
- `parse_api_family` and token normalization helpers
- parser functions such as `parse_term` or `parse_primary`
- private mapping helpers such as `resolve_dimension` and `resolve_tiered`

## Sensitive Fields

Do not log raw `UsageEvent` metadata from this crate. It can contain provider,
model, request metadata, and key identifiers. The enrichment code explicitly
writes provider API key ID into the settlement snapshot, so raw metadata should
be treated as sensitive operational data.

```rust
// crates/aether-billing/src/event_enrichment.rs:247
"pricing_snapshot": {
    "provider_id": pricing.provider_id.clone(),
    "provider_billing_type": pricing.provider_billing_type.clone(),
    "provider_api_key_id": pricing.provider_api_key_id.clone(),
```

If a caller needs logs, log stable identifiers and status only, for example
request ID, billing status, pricing source, and whether the event was charged.
Avoid logging full pricing JSON or full `request_metadata`.

## Suggested Levels If A Caller Logs

These levels are for caller crates, not for `aether-billing` internals:

- `debug`: successful billing status and pricing source during local diagnosis
- `info`: billing enrichment completed for a durable background settlement step
- `warn`: context missing for a completed billable event when policy expects one
- `error`: data-layer lookup failure or metadata serialization failure

The crate already makes these states observable without logs through return
values:

```rust
// crates/aether-billing/src/event_enrichment.rs:37
) -> Result<(), DataLayerError> {
```

```rust
// crates/aether-billing/src/schema.rs:83
pub struct CostResult {
    pub cost: f64,
    pub status: BillingSnapshotStatus,
    pub snapshot: BillingSnapshot,
}
```

## Do Not

Do not add `println!` or `eprintln!` for billing diagnostics. Tests should assert
on returned snapshots and metadata.

Do not log every formula variable or billing dimension by default. That would
duplicate persisted billing metadata and can leak request-specific usage data.

Do not add a `tracing` dependency only for pure calculation paths. Add logging in
the caller crate that has request context, sampling policy, and redaction rules.
