# Quality Guidelines

`aether-billing` quality is mostly about deterministic money math, stable JSON
snapshot schemas, and narrow boundaries. The crate should remain easy to test
with plain Rust unit tests plus async tests for the enrichment adapter.

## Deterministic Calculation

All cost values must pass through the precision helpers before leaving the
engine or applying actual charges. Storage precision is 8 decimal places;
display precision is 6 decimal places.

```rust
// crates/aether-billing/src/precision.rs:1
pub const BILLING_STORAGE_PRECISION: u32 = 8;
pub const BILLING_DISPLAY_PRECISION: u32 = 6;

pub fn quantize_cost(value: f64) -> f64 {
    quantize_value(value, BILLING_STORAGE_PRECISION)
}
```

`FormulaEngine::evaluate` quantizes both total cost and each `_cost` breakdown
entry.

```rust
// crates/aether-billing/src/formula_engine.rs:181
let mut breakdown = BTreeMap::new();
for (key, value) in &resolved {
    if key.ends_with("_cost") {
        if let Some(number) = as_f64(value) {
            breakdown.insert(key.clone(), quantize_cost(number));
        }
    }
}
```

`BillingService::calculate` applies rate multipliers only after checking
complete status and free-tier policy.

```rust
// crates/aether-billing/src/service.rs:81
let rate_multiplier = pricing.rate_multiplier_for_api_format(input.api_format.as_deref());
let is_free_tier = pricing.is_free_tier();
let actual_total_cost = if is_free_tier {
    0.0
} else {
    quantize_cost(total_cost * rate_multiplier)
};
```

## Stable Snapshot Contracts

Billing snapshots have explicit schema versions. When changing stored metadata,
update the relevant schema version and tests in the same change.

```rust
// crates/aether-billing/src/schema.rs:3
pub const BILLING_SNAPSHOT_SCHEMA_VERSION: &str = "2.0";
```

```rust
// crates/aether-billing/src/event_enrichment.rs:12
const SETTLEMENT_SNAPSHOT_SCHEMA_VERSION: &str = "3.0";
```

Status values are serde `snake_case` and have a matching display string. Add new
statuses in `BillingSnapshotStatus`, `as_str`, and tests together.

```rust
// crates/aether-billing/src/schema.rs:7
#[serde(rename_all = "snake_case")]
pub enum BillingSnapshotStatus {
    Complete,
    Incomplete,
    NoRule,
    Legacy,
}
```

## Type Safety and Visibility

Use concrete snapshot structs for public data contracts instead of loose maps.
The crate accepts JSON for flexible pricing data, but public inputs and outputs
are typed.

```rust
// crates/aether-billing/src/pricing.rs:5
pub struct BillingModelPricingSnapshot {
    pub provider_id: String,
    pub provider_billing_type: Option<String>,
    pub provider_api_key_rate_multipliers: Option<Value>,
    pub default_tiered_pricing: Option<Value>,
    pub model_tiered_pricing: Option<Value>,
}
```

Keep helper functions private unless consumers need them. Examples:
`build_dimensions`, `now_marker`, `has_tiered_pricing_tiers`, `parse_api_family`,
and `resolve_mapping` are private implementation details.

```rust
// crates/aether-billing/src/service.rs:126
fn build_dimensions(input: &BillingUsageInput) -> BTreeMap<String, Value> {
```

```rust
// crates/aether-billing/src/token_normalization.rs:9
fn parse_api_family(api_format: Option<&str>) -> ApiFamily {
```

Use `BTreeMap` for billing dimensions, variables, and cost breakdowns so
snapshot JSON remains stable and test-friendly.

```rust
// crates/aether-billing/src/schema.rs:32
pub struct BillingSnapshot {
    pub resolved_dimensions: BTreeMap<String, serde_json::Value>,
    pub resolved_variables: BTreeMap<String, serde_json::Value>,
    pub cost_breakdown: BTreeMap<String, f64>,
```

## Token Normalization Rules

API-family differences belong in `token_normalization.rs`. Do not inline
OpenAI/Claude/Gemini cache-token behavior into billing formulas or service code.

```rust
// crates/aether-billing/src/token_normalization.rs:27
pub fn normalize_input_tokens_for_billing(
    api_format: Option<&str>,
    input_tokens: i64,
    cache_read_tokens: i64,
) -> i64 {
```

OpenAI and Gemini subtract cache-read tokens from billable input tokens; Claude
keeps input tokens because Claude-style usage already reports them differently.

```rust
// crates/aether-billing/src/token_normalization.rs:37
match parse_api_family(api_format) {
    ApiFamily::Claude => input_tokens,
    ApiFamily::OpenAi | ApiFamily::Gemini => (input_tokens - cache_read_tokens).max(0),
    ApiFamily::Unknown => input_tokens,
}
```

## Formula Safety

The formula language is intentionally restricted to arithmetic tokens and a
small function allowlist. Add operations by extending tokenizer, parser, and
tests together.

```rust
// crates/aether-billing/src/formula_engine.rs:709
fn evaluate_function(name: &str, args: &[f64]) -> Result<f64, UnsafeExpressionError> {
    match name {
        "min" => args
            .iter()
            .copied()
            .reduce(f64::min)
```

Do not evaluate billing expressions with a general-purpose scripting engine,
`eval`, shell commands, or dynamic code loading. The current parser returns
`UnsafeExpressionError::Unsupported` for syntax it does not understand.

```rust
// crates/aether-billing/src/formula_engine.rs:542
return Err(UnsafeExpressionError::Unsupported(format!(
    "unsupported character in expression: {ch}"
)));
```

## Testing Requirements

Add or update tests in the same module as the behavior. This crate currently
uses module-local tests and `#[tokio::test]` only for async enrichment.

```rust
// crates/aether-billing/src/service.rs:242
#[test]
fn calculates_complete_snapshot_for_usage() {
    let result = BillingService::new()
        .calculate(
            &pricing(),
```

```rust
// crates/aether-billing/src/event_enrichment.rs:319
#[tokio::test]
async fn enriches_completed_usage_event_with_billing_snapshot() {
```

Tests should cover:

- complete snapshots with non-zero cost
- free-tier and rate multiplier behavior
- `NoRule` and `Incomplete` status behavior
- tiered pricing, TTL pricing, and fallback pricing
- API-family token normalization
- event enrichment lookup priority and metadata shape

Before changing this crate, run:

```bash
cargo test -p aether-billing
```

## Do Not

Do not add new dependencies for basic arithmetic, rounding, expression parsing,
or logging without a clear crate-level reason. The current dependency set is
small and workspace-managed.

Do not use `HashMap` for persisted snapshot fields where deterministic ordering
matters. Prefer `BTreeMap` as current snapshots do.

Do not hide money-math behavior in tests only. If a pricing rule changes, make
the production mapping explicit in `default_rule.rs` or `pricing.rs`.

Do not mutate `UsageEvent` outside `event_enrichment.rs`; service and formula
modules should remain side-effect free.
