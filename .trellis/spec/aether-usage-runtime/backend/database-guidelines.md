# Database Guidelines

> Persistence-boundary rules for the `aether-usage-runtime` backend crate.

## Overview

`aether-usage-runtime` participates in persistence, but it does not own concrete database clients, SeaORM entities, sqlx queries, migrations, or Redis connections. It depends on contracts:

* `aether_runtime_state::RuntimeQueueStore` for queue-like usage event transport
* `aether_data_contracts::repository::usage::UpsertUsageRecord` for usage record writes
* `aether_data_contracts::repository::settlement::UsageSettlementInput` for settlement writes
* crate-local traits (`UsageRecordWriter`, `UsageSettlementWriter`, `ManualProxyNodeCounter`) for injected data side effects

Keep concrete repository implementations in data/gateway crates. This crate should remain a persistence orchestrator and payload builder.

## Queue Persistence Pattern

`UsageQueue` wraps `RuntimeQueueStore` and exposes only usage-specific operations. It should not know whether the backing store is Redis, memory, or another runtime queue implementation.

```rust
// crates/aether-usage-runtime/src/queue.rs:21
pub fn new(
    runner: Arc<dyn RuntimeQueueStore>,
    config: UsageRuntimeConfig,
) -> Result<Self, DataLayerError> {
    config.validate()?;
    Ok(Self {
        runner,
        stream: config.stream_key.clone(),
        group: config.consumer_group.clone(),
        dlq_stream: config.dlq_stream_key.clone(),
        config,
    })
}
```

Queue writes must serialize a `UsageEvent` into stream fields through `UsageEvent::to_stream_fields`; do not build queue payload maps by hand.

```rust
// crates/aether-usage-runtime/src/queue.rs:41
pub async fn enqueue(&self, event: &UsageEvent) -> Result<String, DataLayerError> {
    let fields = event.to_stream_fields()?;
    self.runner
        .append_fields_with_maxlen(&self.stream, &fields, Some(self.config.stream_maxlen))
        .await
}
```

Consumer reads must honor runtime config for batch size and blocking timeout.

```rust
// crates/aether-usage-runtime/src/queue.rs:52
self.runner
    .read_group(
        &self.stream,
        &self.group,
        consumer,
        self.config.consumer_batch_size.max(1),
        Some(self.config.consumer_block_ms.max(1)),
    )
    .await
```

## Write Contract Pattern

`UsageRecordWriter` is the record persistence boundary. It returns the stored usage audit when downstream settlement may need it.

```rust
// crates/aether-usage-runtime/src/worker.rs:32
#[async_trait]
pub trait UsageRecordWriter: Send + Sync {
    async fn upsert_usage_record(
        &self,
        record: UpsertUsageRecord,
    ) -> Result<Option<StoredRequestUsageAudit>, DataLayerError>;
}
```

The gateway implements the trait by delegating to `GatewayDataState`, keeping the concrete repository outside this crate.

```rust
// apps/aether-gateway/src/data/state/integrations.rs:299
impl UsageRecordWriter for GatewayDataState {
    async fn upsert_usage_record(
        &self,
        record: UpsertUsageRecord,
    ) -> Result<Option<StoredRequestUsageAudit>, DataLayerError> {
        GatewayDataState::upsert_usage(self, record).await
    }
}
```

## Settlement Pattern

Settlement is a second injected persistence boundary. Usage runtime only decides whether settlement should be attempted and builds `UsageSettlementInput`.

```rust
// crates/aether-usage-runtime/src/settlement.rs:16
pub async fn settle_usage_if_needed(
    writer: &dyn UsageSettlementWriter,
    usage: &StoredRequestUsageAudit,
) -> Result<(), DataLayerError> {
    if !writer.has_usage_settlement_writer() || usage.billing_status != "pending" {
        return Ok(());
    }
    if !matches!(usage.status.as_str(), "completed" | "failed" | "cancelled") {
        return Ok(());
    }
    ...
}
```

The cost fields must be finite before any settlement write.

```rust
// crates/aether-usage-runtime/src/settlement.rs:38
total_cost_usd: finite_cost(usage.total_cost_usd)?,
actual_total_cost_usd: finite_cost(usage.actual_total_cost_usd)?,
```

## Direct Write vs Queue Write

Terminal events normally enqueue when `UsageRuntimeAccess::usage_worker_queue` returns a queue runner. If the queue cannot be created or enqueue fails, runtime falls back to direct repository writes.

```rust
// crates/aether-usage-runtime/src/runtime.rs:351
async fn enqueue_or_write_terminal<T>(&self, data: &T, event: UsageEvent)
where
    T: UsageRuntimeAccess,
{
    if let Some(runner) = data.usage_worker_queue() {
        ...
    }

    self.write_terminal_direct(data, &event).await;
}
```

This fallback is important for local memory state, degraded queue state, and tests. Do not remove it just because the queue path exists.

## Data Shape Rules

Always map `UsageEvent` to `UpsertUsageRecord` through `build_upsert_usage_record_from_event`.

```rust
// crates/aether-usage-runtime/src/worker.rs:221
let record = build_upsert_usage_record_from_event(event)?;
if let Some(stored) = data.upsert_usage_record(record).await? {
    settle_usage_if_needed(data, &stored).await?;
}
```

`record.rs` maps first-class columns directly and sanitizes metadata before storing it.

```rust
// crates/aether-usage-runtime/src/record.rs:126
request_metadata: sanitize_usage_request_metadata(data.request_metadata),
finalized_at_unix_secs: Some(now_unix_secs),
created_at_unix_ms: Some(now_unix_secs),
updated_at_unix_secs: now_unix_secs,
```

Do not store routing identifiers only in metadata when they have first-class columns. `candidate_id`, `candidate_index`, `key_name`, `route_family`, and related fields are copied onto `UpsertUsageRecord`.

## Migrations

Do not add migrations in this crate. Schema changes belong in data-schema/data repositories and in the database migration system used by the gateway.

When a usage field changes shape, update this crate only after the data contract type already supports it. The order is:

1. data contract and migration
2. event data / seed builders
3. record mapping
4. gateway/admin read or display paths
5. tests for the full write/read behavior

## Configuration

Queue settings are validated in `UsageRuntimeConfig`, and gateway CLI/env arguments are normalized before constructing the runtime.

```rust
// apps/aether-gateway/src/main.rs:493
impl GatewayUsageArgs {
    fn to_config(&self) -> UsageRuntimeConfig {
        UsageRuntimeConfig {
            enabled: true,
            stream_key: self.queue_stream_key.trim().to_string(),
            consumer_group: self.queue_group.trim().to_string(),
            dlq_stream_key: self.queue_dlq_stream_key.trim().to_string(),
            stream_maxlen: self.queue_stream_maxlen.max(1),
            consumer_batch_size: self.queue_batch_size.max(1),
            consumer_block_ms: self.queue_block_ms.max(1),
            reclaim_idle_ms: self.queue_reclaim_idle_ms.max(1),
            reclaim_count: self.queue_reclaim_count.max(1),
            reclaim_interval_ms: self.queue_reclaim_interval_ms.max(1),
        }
    }
}
```

## Common Mistakes

DON'T import SeaORM, sqlx, Redis clients, or migration types into `aether-usage-runtime`.

DON'T write repository queries in `write.rs` or `record.rs`; those modules are pure transformation layers.

DON'T acknowledge queue entries before the event has been recorded or dead-lettered.

DON'T persist raw metadata blobs. All metadata must go through `sanitize_usage_request_metadata`.

DON'T drop the `Option<StoredRequestUsageAudit>` returned by `upsert_usage_record` if settlement may be needed.
