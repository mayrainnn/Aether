# Directory Structure

`aether-billing` is a small service/domain crate for billing calculation,
pricing snapshots, formula evaluation, token normalization, and usage-event
enrichment. It does not expose HTTP routes, SeaORM entities, migrations, or
runtime state. Keep new code in this crate as deterministic billing-domain
logic and let caller crates own persistence, request routing, and observability.

## Current Layout

The public crate boundary is centralized in `src/lib.rs`; implementation files
stay private unless the type or function is re-exported there.

```rust
// crates/aether-billing/src/lib.rs:1
mod default_rule;
mod event_enrichment;
mod formula_engine;
mod models;
mod precision;
mod pricing;
mod schema;
mod service;
mod token_normalization;
```

```text
crates/aether-billing/
  Cargo.toml
  src/
    lib.rs                  public API re-exports only
    default_rule.rs         virtual rule generation from pricing snapshots
    event_enrichment.rs     mutates UsageEvent metadata with billing snapshots
    formula_engine.rs       safe arithmetic expression evaluator
    models.rs               low-level units and dimensions
    precision.rs            billing/display rounding helpers
    pricing.rs              pricing and usage input snapshots
    schema.rs               persisted billing snapshot schema types
    service.rs              BillingService orchestration
    token_normalization.rs  API-family token normalization helpers
```

## Public Boundary

Use `lib.rs` as the only public surface for consumers. Internal modules remain
private with `mod`, then selected types/functions are re-exported with `pub use`.
This keeps downstream crates from depending on implementation filenames.

```rust
// crates/aether-billing/src/lib.rs:15
pub use event_enrichment::{enrich_usage_event_with_billing, BillingModelContextLookup};
pub use formula_engine::{
    extract_variable_names, BillingIncompleteError, ExpressionEvaluationError, FormulaEngine,
    FormulaEvaluationResult, FormulaEvaluationStatus, UnsafeExpressionError,
};
pub use pricing::{BillingComputation, BillingModelPricingSnapshot, BillingUsageInput};
pub use service::BillingService;
```

Add new public items by first deciding whether they belong in an existing module
and then re-exporting them from `lib.rs`. Do not make modules themselves public
just to reach one helper.

## Module Responsibilities

`service.rs` owns orchestration. `BillingService::calculate` generates a virtual
rule, builds normalized dimensions, evaluates the formula, applies free-tier and
rate-multiplier policy, and returns a `BillingComputation`.

```rust
// crates/aether-billing/src/service.rs:21
impl BillingService {
    pub fn new() -> Self {
        Self {
            engine: FormulaEngine::new(),
        }
    }

    pub fn calculate(
        &self,
        pricing: &BillingModelPricingSnapshot,
        input: &BillingUsageInput,
    ) -> Result<BillingComputation, ExpressionEvaluationError> {
```

`default_rule.rs` converts effective model pricing into a generated
`VirtualBillingRule`. It is the place for pricing dimension mappings, tier
resolution metadata, and cache TTL pricing defaults.

```rust
// crates/aether-billing/src/default_rule.rs:21
impl DefaultBillingRuleGenerator {
    pub fn generate_for_pricing(
        pricing: &BillingModelPricingSnapshot,
        task_type: &str,
    ) -> Option<VirtualBillingRule> {
```

`formula_engine.rs` is intentionally self-contained. It tokenizes and evaluates
a restricted arithmetic language, resolves mappings, and returns structured
incomplete states instead of panicking or evaluating arbitrary code.

```rust
// crates/aether-billing/src/formula_engine.rs:61
pub fn evaluate(
    &self,
    expression: &str,
    variables: Option<&BTreeMap<String, serde_json::Value>>,
    dimensions: Option<&BTreeMap<String, serde_json::Value>>,
    dimension_mappings: Option<&BTreeMap<String, serde_json::Value>>,
    strict_mode: bool,
) -> Result<FormulaEvaluationResult, ExpressionEvaluationError> {
```

`event_enrichment.rs` is the integration adapter. It accepts a trait object for
billing context lookup and mutates `UsageEvent` fields plus request metadata.
Persistence still lives outside this crate.

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

`pricing.rs`, `schema.rs`, `models.rs`, `precision.rs`, and
`token_normalization.rs` are value/type/helper modules. Keep them free of
lookup traits and side effects.

## Naming and Placement Rules

Use file names that describe billing concepts, not infrastructure. Current
names are domain nouns: `pricing`, `schema`, `precision`,
`token_normalization`, and `event_enrichment`.

Put formula-language changes in `formula_engine.rs`, not in `service.rs`.
`BillingService` should call the engine; it should not grow parser or evaluator
logic.

Put API-family token math in `token_normalization.rs`. `build_dimensions` in
`service.rs` calls the normalizer before constructing formula dimensions.

```rust
// crates/aether-billing/src/service.rs:126
fn build_dimensions(input: &BillingUsageInput) -> BTreeMap<String, Value> {
    let normalized_input_tokens = normalize_input_tokens_for_billing(
        input.api_format.as_deref(),
        input.input_tokens,
        input.cache_read_tokens,
    );
```

## Dependency Boundaries

`Cargo.toml` only depends on shared contracts/runtime crates plus generic Rust
support libraries. Do not add database or web framework dependencies to this
crate.

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

GitNexus reports Aether as a 3,140-file, 83,229-symbol repo with `Usage`,
`Wallet`, `Payments`, and `Services` clusters. ABCoder's AST graph for
`aether-billing` shows the local dependency path
`BillingService.calculate -> DefaultBillingRuleGenerator::generate_for_pricing
-> FormulaEngine.evaluate`, with `event_enrichment` depending on the service
through `calculate_billing_computation`.

## Do Not

Do not add route handlers, axum extractors, SeaORM entities, Redis clients, or
background workers here. Put those in application/data/runtime crates and pass
the billing crate the already-loaded data it needs.

Do not expose implementation modules with `pub mod`. Export stable types and
functions from `lib.rs` instead.

Do not split one billing concept across multiple files before there is real
pressure. For example, cache token normalization belongs beside the API-family
normalizer unless it grows independent state or external contracts.
