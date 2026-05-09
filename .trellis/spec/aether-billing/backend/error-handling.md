# Error Handling

`aether-billing` uses typed, local errors for formula safety and converts those
errors to caller-layer errors only at the usage-event enrichment boundary. Pure
calculation APIs return structured `Result` values or non-error status enums;
they do not panic for missing pricing, missing dimensions, or partial billing
inputs.

## Error Types

Formula parsing and evaluation errors live in `formula_engine.rs` and derive
`thiserror::Error`.

```rust
// crates/aether-billing/src/formula_engine.rs:27
#[derive(Debug, Error)]
pub enum UnsafeExpressionError {
    #[error("unsupported expression syntax: {0}")]
    Unsupported(String),
}

#[derive(Debug, Error)]
pub enum ExpressionEvaluationError {
    #[error("expression evaluation failed: {0}")]
    Failed(String),
}

#[derive(Debug, Error)]
#[error("missing required dimensions: {missing_required:?}")]
pub struct BillingIncompleteError {
    pub missing_required: Vec<String>,
}
```

`UnsafeExpressionError` is for the restricted arithmetic language: tokenization,
unknown variables, unsupported functions, malformed calls, and invalid numeric
literals. `ExpressionEvaluationError` is the public error returned by
`FormulaEngine::evaluate` and `BillingService::calculate`.

The enrichment boundary uses `aether_data_contracts::DataLayerError` because it
is called by data/integration code that already speaks that error type.

```rust
// crates/aether-billing/src/event_enrichment.rs:2
use aether_data_contracts::DataLayerError;
```

## Propagation Pattern

Within the formula engine, helper errors are propagated with `?` and wrapped
only when crossing from unsafe-expression parsing into public evaluation errors.

```rust
// crates/aether-billing/src/formula_engine.rs:91
let (value, is_missing, tier_meta) = resolve_mapping(var_name, mapping, &dims)?;
```

```rust
// crates/aether-billing/src/formula_engine.rs:165
let cost = evaluate_expression(expression, &resolved)
    .map_err(|err| ExpressionEvaluationError::Failed(err.to_string()))?;
```

`BillingService::calculate` preserves `ExpressionEvaluationError` directly. It
does not translate formula failures into strings, HTTP responses, or data-layer
errors.

```rust
// crates/aether-billing/src/service.rs:27
pub fn calculate(
    &self,
    pricing: &BillingModelPricingSnapshot,
    input: &BillingUsageInput,
) -> Result<BillingComputation, ExpressionEvaluationError> {
```

Only `event_enrichment.rs` converts billing-calculation failures into
`DataLayerError::UnexpectedValue`, because that function also calls the external
lookup trait and returns a single integration-layer error type.

```rust
// crates/aether-billing/src/event_enrichment.rs:156
BillingService::new()
    .calculate(pricing, &input)
    .map_err(|err| {
        DataLayerError::UnexpectedValue(format!("billing calculation failed: {err}"))
    })
```

## Incomplete Is Usually Not An Error

Missing dimensions are represented as `FormulaEvaluationStatus::Incomplete`
unless the caller explicitly enables strict mode. This is important for billing
snapshots: callers need to persist why calculation was incomplete instead of
dropping the event.

```rust
// crates/aether-billing/src/formula_engine.rs:146
if !missing_required.is_empty() {
    if strict_mode {
        return Err(ExpressionEvaluationError::Failed(
            BillingIncompleteError { missing_required }.to_string(),
        ));
    }
    return Ok(FormulaEvaluationResult {
        status: FormulaEvaluationStatus::Incomplete,
        cost: 0.0,
```

`BillingService::calculate` mirrors that status into the billing snapshot and
only charges when the snapshot is complete.

```rust
// crates/aether-billing/src/service.rs:74
let status = match result.status {
    FormulaEvaluationStatus::Complete => BillingSnapshotStatus::Complete,
    FormulaEvaluationStatus::Incomplete => BillingSnapshotStatus::Incomplete,
};
let total_cost = if matches!(status, BillingSnapshotStatus::Complete) {
    result.cost
} else {
    0.0
};
```

When no billing rule can be generated from pricing, return a successful
`BillingComputation` with `BillingSnapshotStatus::NoRule`. This is not an
exceptional error condition.

```rust
// crates/aether-billing/src/service.rs:33
else {
    return Ok(BillingComputation {
        cost_result: CostResult {
            cost: 0.0,
            status: BillingSnapshotStatus::NoRule,
```

## Integration Boundary Rules

`enrich_usage_event_with_billing` short-circuits non-completed events to zero
cost and returns `Ok(())`. Billing failures should not be invented for skipped,
failed, or in-progress usage events.

```rust
// crates/aether-billing/src/event_enrichment.rs:38
if !matches!(event.event_type, UsageEventType::Completed) {
    event.data.total_cost_usd = Some(0.0);
    event.data.actual_total_cost_usd = Some(0.0);
    return Ok(());
}
```

Lookup errors propagate with `await?` because callers need to know when the
data layer failed. Missing contexts are `Ok(None)` and simply move to the next
lookup candidate.

```rust
// crates/aether-billing/src/event_enrichment.rs:61
if let Some(context) = data
    .find_billing_model_context_by_model_id(
        provider_id,
        event.data.provider_api_key_id.as_deref(),
        model_id,
    )
    .await?
{
```

Metadata serialization errors are converted to `UnexpectedValue` with context.

```rust
// crates/aether-billing/src/event_enrichment.rs:210
let billing_snapshot = serde_json::to_value(snapshot).map_err(|err| {
    DataLayerError::UnexpectedValue(format!("failed to serialize billing snapshot: {err}"))
})?;
```

## Do Not

Do not use `unwrap()` or `expect()` in production billing paths. Tests use
`expect("billing should calculate")`, but runtime code returns `Result` or a
status enum.

Do not make `Incomplete` or `NoRule` fatal by default. They are persisted
billing states and are part of the snapshot contract.

Do not convert errors to strings inside pure modules unless crossing an error
type boundary. Keep typed errors until the integration layer forces conversion.

Do not swallow lookup errors from `BillingModelContextLookup`. Missing context is
`Ok(None)`; failed context lookup is `Err(DataLayerError)` and should propagate.
