# Directory Structure

> Module layout and ownership rules for `crates/aether-wallet`.

---

## Overview

`aether-wallet` is a foundation-layer Rust crate for wallet and provider-quota
value logic. It is deliberately small: it has no internal Aether crate
dependencies, no database adapters, no async runtime, and no logging surface.
The crate is a stable facade over pure value types that higher layers map from
their storage records.

GitNexus places wallet-related code in the broader `Wallet` functional cluster,
but this crate is the leaf calculation surface inside that cluster. Repository
and API workflows live in `crates/aether-data` and `apps/aether-gateway`; this
crate owns only the business decisions that can be expressed without I/O.

## Actual Layout

```text
crates/aether-wallet/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── access.rs
    └── quota.rs
```

`Cargo.toml` declares only `serde.workspace = true`. Keep that dependency shape
unless a new value type truly needs another serialization-compatible primitive.

## Module Responsibilities

`src/lib.rs` is the public facade. It keeps implementation modules private and
re-exports only the crate contract:

```rust
// crates/aether-wallet/src/lib.rs:1-8
mod access;
mod quota;

pub use access::{
    quantize_money, WalletAccessDecision, WalletAccessFailure, WalletLimitMode, WalletSnapshot,
    WalletStatus,
};
pub use quota::{ProviderBillingType, ProviderQuotaSnapshot};
```

`src/access.rs` owns wallet balance and access decisions. It defines
`WalletLimitMode`, `WalletStatus`, `WalletSnapshot`, `WalletAccessFailure`,
`WalletAccessDecision`, and `quantize_money`. The decision order in
`WalletSnapshot::access_decision` is the core API contract: admin bypass,
inactive wallet denial, negative recharge denial, unlimited allowance, then
finite balance check.

```rust
// crates/aether-wallet/src/access.rs:66-85
pub fn access_decision(&self, is_admin: bool) -> WalletAccessDecision {
    if is_admin {
        return WalletAccessDecision::allowed(None);
    }
    if self.status != WalletStatus::Active {
        return WalletAccessDecision::wallet_unavailable(self.balance_snapshot());
    }
    if self.recharge_balance < 0.0 {
        return WalletAccessDecision::balance_denied(Some(quantize_money(
            self.recharge_balance,
        )));
    }
    if self.limit_mode == WalletLimitMode::Unlimited {
        return WalletAccessDecision::allowed(None);
    }
    let remaining = self.spendable_balance();
    if remaining <= 0.0 {
        return WalletAccessDecision::balance_denied(Some(remaining));
    }
    WalletAccessDecision::allowed(Some(remaining))
}
```

`src/quota.rs` owns provider quota classification and reset predicates. It
imports `crate::quantize_money` instead of duplicating money rounding.

```rust
// crates/aether-wallet/src/quota.rs:36-58
impl ProviderQuotaSnapshot {
    pub fn remaining_quota_usd(&self) -> Option<f64> {
        self.monthly_quota_usd
            .map(|quota| quantize_money(quota - self.monthly_used_usd))
    }

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
}
```

## Boundary With Storage and Gateway Code

Storage records stay outside this crate. `StoredWalletSnapshot` keeps
database-facing strings and extra accounting totals in `crates/aether-data`,
then the gateway maps those records into `aether-wallet` value types:

```rust
// apps/aether-gateway/src/wallet_runtime/access.rs:53-63
fn map_wallet_snapshot(snapshot: &StoredWalletSnapshot) -> WalletSnapshot {
    WalletSnapshot {
        wallet_id: snapshot.id.clone(),
        user_id: snapshot.user_id.clone(),
        api_key_id: snapshot.api_key_id.clone(),
        recharge_balance: snapshot.balance,
        gift_balance: snapshot.gift_balance,
        limit_mode: WalletLimitMode::parse(&snapshot.limit_mode),
        currency: snapshot.currency.clone(),
        status: WalletStatus::parse(&snapshot.status),
    }
}
```

Quota storage follows the same boundary. Repositories construct
`ProviderQuotaSnapshot` from stored string fields, then call pure predicates:

```rust
// crates/aether-data/src/repository/quota/memory.rs:63-80
for quota in quotas.values_mut() {
    let snapshot = ProviderQuotaSnapshot {
        provider_id: quota.provider_id.clone(),
        billing_type: ProviderBillingType::parse(&quota.billing_type),
        monthly_quota_usd: quota.monthly_quota_usd,
        monthly_used_usd: quota.monthly_used_usd,
        quota_reset_day: quota.quota_reset_day,
        quota_last_reset_at_unix_secs: quota.quota_last_reset_at_unix_secs,
        quota_expires_at_unix_secs: quota.quota_expires_at_unix_secs,
        is_active: quota.is_active,
    };
    if snapshot.should_reset(now_unix_secs) {
        quota.monthly_used_usd = 0.0;
        quota.quota_last_reset_at_unix_secs = Some(now_unix_secs);
        count += 1;
    }
}
```

## Where New Code Goes

Add new wallet access rules to `access.rs` when they can be decided from a
`WalletSnapshot` alone. Add provider quota rules to `quota.rs` when they can be
decided from `ProviderQuotaSnapshot` alone. Add a new module only if the concept
is neither wallet access nor provider quota and it will be re-exported from
`lib.rs`.

Do not add route handlers, repository traits, SQL mapping, background workers,
or admin payload shapes to this crate. Those belong to `apps/aether-gateway`,
`crates/aether-data`, or `crates/aether-data-contracts`.

## Naming Conventions

Use domain nouns for data containers: `WalletSnapshot`,
`ProviderQuotaSnapshot`. Use outcome nouns for decisions:
`WalletAccessDecision`, `WalletAccessFailure`. Use `parse` for tolerant string
normalization at storage boundaries. Use predicate names for pure boolean
checks, such as `is_expired` and `should_reset`.

File names stay singular by domain: `access.rs` and `quota.rs`, not
`wallet_access.rs` or `provider_quota.rs`. The crate name already scopes the
files.

## Tests Placement

Unit tests live inline in the module they protect. `access.rs` keeps wallet
decision tests near `WalletSnapshot`; `quota.rs` keeps quota reset tests near
`ProviderQuotaSnapshot`. Use private helpers inside the `#[cfg(test)]` module
when constructing repeated snapshots.

```rust
// crates/aether-wallet/src/access.rs:132-151
#[cfg(test)]
mod tests {
    use super::{WalletAccessFailure, WalletLimitMode, WalletSnapshot, WalletStatus};

    fn wallet_snapshot(limit_mode: WalletLimitMode, recharge: f64, gift: f64) -> WalletSnapshot {
        WalletSnapshot {
            wallet_id: "wallet-1".to_string(),
            user_id: Some("user-1".to_string()),
            api_key_id: None,
            recharge_balance: recharge,
            gift_balance: gift,
            limit_mode,
            currency: "USD".to_string(),
            status: WalletStatus::Active,
        }
    }
}
```
