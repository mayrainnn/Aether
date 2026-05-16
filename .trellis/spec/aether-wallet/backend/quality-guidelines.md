# Quality Guidelines

> Code quality standards for `crates/aether-wallet`.

---

## Overview

`aether-wallet` should remain a small, dependency-light, pure Rust crate. It is
the shared wallet decision surface used by gateway runtime, data repositories,
and scheduler code. Quality here means stable value semantics, conservative
defaults, reproducible money math, and tests that lock every branch.

The crate currently compiles with Rust 2021 workspace settings and depends only
on `serde`. Do not add a new dependency for convenience. If a rule needs I/O,
clock access beyond a caller-provided timestamp, database queries, or logging,
the rule probably belongs in a higher layer.

## Public API Shape

Keep modules private and re-export public types from `lib.rs`. This gives
callers one stable import surface and keeps internal file names free to change.

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

Use `pub` only for the crate contract. Helper functions inside tests stay
private. If a helper is needed by both `access.rs` and `quota.rs`, put it in the
owning domain module and import through `crate::...`; `quota.rs` already does
this for money quantization.

```rust
// crates/aether-wallet/src/quota.rs:1-4
use serde::{Deserialize, Serialize};

use crate::quantize_money;
```

## Type Safety Patterns

Small enums derive `Copy` and `Eq` when they are closed value categories. Larger
snapshots derive `Clone` and `PartialEq` for tests and caller payload assembly.
All public value types derive `Serialize` and `Deserialize` because snapshots
can cross API, cache, or test boundaries.

```rust
// crates/aether-wallet/src/quota.rs:5-11
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderBillingType {
    MonthlyQuota,
    PayAsYouGo,
    FreeTier,
    Unknown,
}
```

Use `Option` to represent semantic absence, not magic numbers. The current API
uses `None` for unlimited wallet remaining balance and for providers that do
not have a monthly quota.

```rust
// crates/aether-wallet/src/access.rs:56-63
pub fn balance_snapshot(&self) -> Option<f64> {
    if self.recharge_balance < 0.0 {
        return Some(quantize_money(self.recharge_balance));
    }
    match self.limit_mode {
        WalletLimitMode::Unlimited => None,
        WalletLimitMode::Finite => Some(self.spendable_balance()),
    }
}
```

## Money and Time Rules

Always quantize monetary outputs through `quantize_money`. This crate uses eight
decimal places for both wallet balances and quota remaining amounts.

```rust
// crates/aether-wallet/src/access.rs:128-130
pub fn quantize_money(value: f64) -> f64 {
    (value * 100_000_000.0).round() / 100_000_000.0
}
```

Do not compare or return derived balances before quantization. Current wallet
helpers quantize `recharge_balance + gift_balance`, refundable balance, and
negative recharge snapshots:

```rust
// crates/aether-wallet/src/access.rs:47-58
impl WalletSnapshot {
    pub fn spendable_balance(&self) -> f64 {
        quantize_money(self.recharge_balance + self.gift_balance)
    }

    pub fn refundable_balance(&self) -> f64 {
        quantize_money(self.recharge_balance)
    }

    pub fn balance_snapshot(&self) -> Option<f64> {
        if self.recharge_balance < 0.0 {
            return Some(quantize_money(self.recharge_balance));
        }
```

Keep time inputs caller-provided and deterministic. `ProviderQuotaSnapshot`
does not read the system clock; callers pass `now_unix_secs`.

```rust
// crates/aether-wallet/src/quota.rs:42-57
pub fn is_expired(&self, now_unix_secs: u64) -> bool {
    self.quota_expires_at_unix_secs
        .is_some_and(|expires_at| expires_at <= now_unix_secs)
}
```

## Required Decision Ordering

Preserve the wallet access decision order unless tests and gateway rejection
mapping are updated together:

1. Admin bypass returns `allowed(None)`.
2. Inactive status returns `WalletUnavailable`.
3. Negative recharge balance returns `BalanceDenied`, even for unlimited mode.
4. Unlimited mode returns `allowed(None)`.
5. Finite wallets require positive spendable balance.

The negative-before-unlimited rule is locked by the existing test:

```rust
// crates/aether-wallet/src/access.rs:157-166
#[test]
fn unlimited_wallet_ignores_balance() {
    let decision =
        wallet_snapshot(WalletLimitMode::Unlimited, -10.0, 0.0).access_decision(false);
    assert!(!decision.allowed);
    assert_eq!(decision.failure, Some(WalletAccessFailure::BalanceDenied));

    let decision = wallet_snapshot(WalletLimitMode::Unlimited, 0.0, 0.0).access_decision(false);
    assert!(decision.allowed);
    assert_eq!(decision.remaining, None);
}
```

## Forbidden Patterns

DON'T add repository traits, SQL rows, axum extractors, or background workers to
this crate. Put I/O code in `crates/aether-data` or `apps/aether-gateway`.

DON'T duplicate string parsing at call sites. The gateway correctly parses
stored strings through the crate helpers:

```rust
// apps/aether-gateway/src/wallet_runtime/access.rs:58-62
recharge_balance: snapshot.balance,
gift_balance: snapshot.gift_balance,
limit_mode: WalletLimitMode::parse(&snapshot.limit_mode),
currency: snapshot.currency.clone(),
status: WalletStatus::parse(&snapshot.status),
```

DON'T introduce direct string comparisons such as
`snapshot.limit_mode == "unlimited"` outside parse helpers. That scatters
case-folding and fallback semantics.

DON'T add `unwrap` or `expect` in production wallet logic. Test construction
may use `expect` when validating fixtures, but public methods should be pure and
panic-free.

DON'T change `remaining: None` to `Some(f64::INFINITY)` for unlimited wallets.
Callers already treat `None` as unlimited. A numeric sentinel would leak
transport-specific assumptions into a value crate.

## Testing Requirements

Every new branch must have a unit test in the same file. For `access.rs`, test
the `WalletAccessDecision` fields, not just the boolean. For `quota.rs`, pass
explicit timestamps and assert both before and after the threshold.

`quota.rs` demonstrates the expected style:

```rust
// crates/aether-wallet/src/quota.rs:65-80
#[test]
fn monthly_quota_resets_after_period() {
    let snapshot = ProviderQuotaSnapshot {
        provider_id: "provider-1".to_string(),
        billing_type: ProviderBillingType::MonthlyQuota,
        monthly_quota_usd: Some(20.0),
        monthly_used_usd: 5.0,
        quota_reset_day: Some(7),
        quota_last_reset_at_unix_secs: Some(1_000),
        quota_expires_at_unix_secs: None,
        is_active: true,
    };

    assert!(!snapshot.should_reset(1_000 + 6 * 24 * 60 * 60));
    assert!(snapshot.should_reset(1_000 + 7 * 24 * 60 * 60));
    assert_eq!(snapshot.remaining_quota_usd(), Some(15.0));
}
```

Run at least `cargo test -p aether-wallet` after edits. If the public decision
contract changes, also run the gateway or scheduler tests that map these value
types.

## Code Review Checklist

Reviewers should confirm that the crate still depends only on `serde`; that new
public types are re-exported from `lib.rs`; that unknown storage strings still
fall back safely; that monetary outputs use `quantize_money`; that no logging,
database, or async code entered the crate; and that tests cover all new
decision states and their serialized data shape.
