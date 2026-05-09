# Error Handling

> Error and denial semantics for `crates/aether-wallet`.

---

## Overview

`aether-wallet` does not define transport errors, repository errors, or
`Result`-returning public APIs. Its public contract is pure value logic:
storage and gateway layers validate I/O, then this crate returns deterministic
decisions. In this crate, denial states are represented as data, not thrown or
propagated as errors.

The only public failure type is `WalletAccessFailure`. Treat it as a wallet
access outcome enum, not as a Rust error type. It intentionally does not
implement `std::error::Error`.

## Failure Types

`WalletAccessFailure` has exactly two states:

```rust
// crates/aether-wallet/src/access.rs:89-99
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WalletAccessFailure {
    WalletUnavailable,
    BalanceDenied,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalletAccessDecision {
    pub allowed: bool,
    pub remaining: Option<f64>,
    pub failure: Option<WalletAccessFailure>,
}
```

Use `WalletUnavailable` when the wallet cannot be used at all, such as inactive
status or a missing non-admin wallet. Use `BalanceDenied` when the wallet exists
but the available paid balance prevents the request.

The constructors keep the `allowed`, `remaining`, and `failure` fields
consistent. Call them instead of constructing a decision manually:

```rust
// crates/aether-wallet/src/access.rs:102-125
impl WalletAccessDecision {
    pub fn allowed(remaining: Option<f64>) -> Self {
        Self {
            allowed: true,
            remaining,
            failure: None,
        }
    }

    pub fn wallet_unavailable(remaining: Option<f64>) -> Self {
        Self {
            allowed: false,
            remaining,
            failure: Some(WalletAccessFailure::WalletUnavailable),
        }
    }

    pub fn balance_denied(remaining: Option<f64>) -> Self {
        Self {
            allowed: false,
            remaining,
            failure: Some(WalletAccessFailure::BalanceDenied),
        }
    }
}
```

## Parse and Normalization Rules

Parsing functions in this crate are tolerant because they sit at the boundary
between string-backed storage records and typed wallet logic. Unknown wallet
limit modes become `Finite`; unknown wallet statuses become `Inactive`; unknown
provider billing types become `Unknown`.

```rust
// crates/aether-wallet/src/access.rs:9-16
impl WalletLimitMode {
    pub fn parse(value: &str) -> Self {
        if value.trim().eq_ignore_ascii_case("unlimited") {
            Self::Unlimited
        } else {
            Self::Finite
        }
    }
}
```

```rust
// crates/aether-wallet/src/quota.rs:13-21
impl ProviderBillingType {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "monthly_quota" => Self::MonthlyQuota,
            "pay_as_you_go" => Self::PayAsYouGo,
            "free_tier" => Self::FreeTier,
            _ => Self::Unknown,
        }
    }
}
```

Do not change these helpers to fail closed with `Result` unless every caller is
updated at the same time. The current behavior is an intentional compatibility
boundary for stored string values.

## Caller Error Propagation

Callers convert wallet decisions into their own error or rejection types. The
gateway maps `WalletAccessFailure` to local authentication rejections:

```rust
// apps/aether-gateway/src/wallet_runtime/access.rs:37-50
pub(crate) fn local_rejection_from_wallet_access(
    decision: &WalletAccessDecision,
) -> Option<GatewayLocalAuthRejection> {
    match decision.failure.as_ref() {
        Some(WalletAccessFailure::WalletUnavailable) => {
            Some(GatewayLocalAuthRejection::WalletUnavailable)
        }
        Some(WalletAccessFailure::BalanceDenied) => {
            Some(GatewayLocalAuthRejection::BalanceDenied {
                remaining: decision.remaining,
            })
        }
        None => None,
    }
}
```

Repository and runtime I/O errors stay in higher layers. For example, the
gateway reads a wallet through `AppState`, propagates the data-layer failure as
`GatewayError`, and only then asks `aether-wallet` for a decision:

```rust
// apps/aether-gateway/src/wallet_runtime/access.rs:18-34
let wallet = state
    .read_wallet_snapshot_for_auth(
        &auth_snapshot.user_id,
        &auth_snapshot.api_key_id,
        auth_snapshot.api_key_is_standalone,
    )
    .await?;
let is_admin = wallet_auth_allows_admin_bypass(
    &auth_snapshot.user_role,
    auth_snapshot.api_key_is_standalone,
);

Ok(Some(match wallet.as_ref() {
    Some(wallet) => map_wallet_snapshot(wallet).access_decision(is_admin),
    None if is_admin => WalletAccessDecision::allowed(None),
    None => WalletAccessDecision::wallet_unavailable(None),
}))
```

## API Error Responses

This crate must not know about HTTP status codes, axum responses, API payloads,
or gateway rejection bodies. Keep API formatting in `apps/aether-gateway`.
`aether-wallet` should return typed state that callers can translate into their
own response model.

## DON'T Patterns

DON'T add `anyhow`, `thiserror`, or `std::error::Error` here for wallet access
denials. That would blur the line between business denials and exceptional
runtime failures.

```rust
// DON'T: this turns a normal wallet denial into an infrastructure error.
pub fn access_decision(&self, is_admin: bool) -> anyhow::Result<()> {
    if self.recharge_balance <= 0.0 {
        anyhow::bail!("balance denied");
    }
    Ok(())
}
```

DON'T panic on unknown storage strings. Current stored values are normalized at
the boundary, and old rows or imported data must not crash gateway auth.

```rust
// DON'T: an unknown database value would panic in auth/runtime paths.
match value {
    "active" => WalletStatus::Active,
    "inactive" => WalletStatus::Inactive,
    _ => panic!("unknown wallet status"),
}
```

DON'T expose negative recharge balances as an unlimited-wallet allowance. The
current decision order intentionally checks negative recharge before unlimited:

```rust
// crates/aether-wallet/src/access.rs:73-80
if self.recharge_balance < 0.0 {
    return WalletAccessDecision::balance_denied(Some(quantize_money(
        self.recharge_balance,
    )));
}
if self.limit_mode == WalletLimitMode::Unlimited {
    return WalletAccessDecision::allowed(None);
}
```

## Review Checklist

Before changing this crate, verify that every new failure state has a consumer
mapping in `apps/aether-gateway/src/wallet_runtime/access.rs`. Verify that
parse helpers still provide safe defaults. Verify that no public API now
requires async, I/O, or a gateway error type. Verify that tests cover the
decision branch, not only the constructor path.
