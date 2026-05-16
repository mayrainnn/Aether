# Logging Guidelines

> Observability boundary for `crates/aether-wallet`.

---

## Overview

`aether-wallet` currently contains no `tracing`, `log`, `warn!`, `info!`,
`debug!`, `error!`, or `trace!` usage. That is the intended design. The crate
contains deterministic value calculations, not runtime orchestration. Logging
belongs to the caller that knows request identity, background task context,
tenant/user visibility, and error policy.

Keep this crate log-free unless its role changes from a pure value crate into a
runtime component. A new log statement in `aether-wallet` is usually a sign that
the code belongs in `apps/aether-gateway`, `crates/aether-data`, or scheduler
runtime instead.

## What This Crate Should Return Instead of Logging

Wallet access uses structured return data. The caller can decide whether and how
to log the denial.

```rust
// crates/aether-wallet/src/access.rs:95-99
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalletAccessDecision {
    pub allowed: bool,
    pub remaining: Option<f64>,
    pub failure: Option<WalletAccessFailure>,
}
```

Quota reset logic uses pure predicates. It does not log expired, inactive, or
not-yet-due providers:

```rust
// crates/aether-wallet/src/quota.rs:47-58
pub fn should_reset(&self, now_unix_secs: u64) -> bool {
    if self.billing_type != ProviderBillingType::MonthlyQuota || !self.is_active {
        return false;
    }
    let Some(reset_day) = self.quota_reset_day.filter(|value| *value > 0) else {
        return false;
    };
    let Some(last_reset) = self.quota_last_reset_at_unix_secs else {
        return true;
    };
    now_unix_secs.saturating_sub(last_reset) >= reset_day.saturating_mul(24 * 60 * 60)
}
```

This keeps repeated scheduler checks and auth checks quiet. High-frequency
business predicates should not emit logs for ordinary false outcomes.

## Caller Logging Pattern

When wallet or quota logic is part of a runtime worker, log in that worker with
the caller's error type and operational context. The provider quota reset worker
is the nearby example:

```rust
// apps/aether-gateway/src/wallet_runtime/quota.rs:27-38
Some(tokio::spawn(async move {
    if let Err(err) = reset_due_provider_quotas_once(&data).await {
        warn!(error = %err, "gateway provider quota reset startup failed");
    }
    let mut interval = tokio::time::interval(QUOTA_RESET_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        interval.tick().await;
        if let Err(err) = reset_due_provider_quotas_once(&data).await {
            warn!(error = %err, "gateway provider quota reset tick failed");
        }
    }
}))
```

Notice that the log is not emitted by `ProviderQuotaSnapshot::should_reset`.
The worker logs only I/O failure from reset execution. A normal "not due" quota
is not logged.

## Log Levels

No log levels are used inside `aether-wallet`.

Use caller-level `warn!` for recoverable runtime failures such as failed
background quota reset ticks. Use caller-level `debug!` only when a request or
worker has enough context to make the message useful. Use caller-level `info!`
for lifecycle events, not per-wallet decisions. Use caller-level `error!` only
when the owning runtime treats the failure as service-impacting and has a clear
owner for remediation.

Do not add log levels to parse helpers. `WalletLimitMode::parse`,
`WalletStatus::parse`, and `ProviderBillingType::parse` are intentionally
tolerant; they can be called on every auth or scheduling path.

## Structured Fields

If a caller logs wallet decisions, prefer structured fields and avoid raw
payload dumps. Good caller fields are stable identifiers and sanitized outcome
values: `wallet_id`, `provider_id`, `failure`, `remaining`, and `error`.

Do not log the whole `WalletSnapshot`. It contains `user_id`, `api_key_id`, and
balances:

```rust
// crates/aether-wallet/src/access.rs:35-44
pub struct WalletSnapshot {
    pub wallet_id: String,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub recharge_balance: f64,
    pub gift_balance: f64,
    pub limit_mode: WalletLimitMode,
    pub currency: String,
    pub status: WalletStatus,
}
```

Do not log complete stored wallet records either. `StoredWalletSnapshot` has
additional accounting totals and owner identifiers in `crates/aether-data`.

## What to Log

Log failures at the boundary where the failure becomes operational. Examples:
failed repository reads, failed quota reset writes, failed payment callbacks,
or a gateway rejection if the owning route explicitly audits that event.

Do not log ordinary balance denial inside `aether-wallet`. It is a valid
business result, returned as `WalletAccessDecision`.

Do not log ordinary provider quota exhaustion inside `aether-wallet`. Scheduler
code can observe `ProviderQuotaSnapshot::remaining_quota_usd()` and decide
whether a provider should be skipped:

```rust
// crates/aether-scheduler-core/src/provider.rs:22-27
match snapshot.billing_type {
    ProviderBillingType::MonthlyQuota | ProviderBillingType::FreeTier => snapshot
        .remaining_quota_usd()
        .is_some_and(|remaining| remaining <= 0.0),
    ProviderBillingType::PayAsYouGo | ProviderBillingType::Unknown => false,
}
```

## What NOT to Log

Never log API keys, bearer tokens, payment callback payloads, or full wallet
snapshots from this crate. Avoid logging `user_id` or `api_key_id` unless the
caller has an audit requirement and can apply its normal privacy policy.

Never log unknown parse values from this crate. A bad string may come from
storage, tests, or imports; logging it in a pure helper would create noisy,
context-free messages. If unknown values need auditing, add that audit in the
repository or gateway layer that can include source table, record id, and
operator context.

## Review Checklist

Before accepting a logging change, ask whether the information could be
returned as structured data instead. If logging is still required, keep the log
outside `aether-wallet`, use caller-owned structured fields, and add a test that
the wallet value logic remains deterministic without observing logs.
