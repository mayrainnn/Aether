# Aether Wallet Backend Guidelines

> Entry point for coding in `crates/aether-wallet`.

---

## Scope

These guidelines apply only to the `aether-wallet` Rust crate:

```text
crates/aether-wallet/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── access.rs
    └── quota.rs
```

This crate is a foundation-layer value crate. It owns wallet access decisions,
money quantization, provider billing type parsing, and quota reset predicates.
It does not own storage, route handlers, background workers, payment callbacks,
admin payloads, migrations, or logging.

The current public surface is re-exported from `src/lib.rs`:

```rust
// crates/aether-wallet/src/lib.rs:4-8
pub use access::{
    quantize_money, WalletAccessDecision, WalletAccessFailure, WalletLimitMode, WalletSnapshot,
    WalletStatus,
};
pub use quota::{ProviderBillingType, ProviderQuotaSnapshot};
```

## Pre-Development Checklist

Read these files before editing this crate:

1. [Directory Structure](./directory-structure.md)
2. [Error Handling](./error-handling.md)
3. [Quality Guidelines](./quality-guidelines.md)
4. [Logging Guidelines](./logging-guidelines.md)

Then inspect the source file you plan to change. For access rules, start with
`crates/aether-wallet/src/access.rs`. For provider quota rules, start with
`crates/aether-wallet/src/quota.rs`. For public API changes, start with
`crates/aether-wallet/src/lib.rs` and then search all callers of the exported
symbol.

## Guidelines Index

| Guide | What it locks down | Required before |
|-------|---------------------|-----------------|
| [Directory Structure](./directory-structure.md) | Private modules, public facade, storage/gateway boundary, test placement | Adding files or moving wallet logic |
| [Error Handling](./error-handling.md) | `WalletAccessDecision` and `WalletAccessFailure` as data outcomes, tolerant parse behavior, caller mapping | Adding denial states or parse behavior |
| [Quality Guidelines](./quality-guidelines.md) | Dependency limits, type derives, money quantization, decision ordering, tests | Any code change in the crate |
| [Logging Guidelines](./logging-guidelines.md) | Log-free value crate boundary and caller-side tracing examples | Adding observability around wallet/quota behavior |

`database-guidelines.md` was intentionally removed for this spec directory.
`aether-wallet` has no database client, ORM, repository trait, SQL mapping, or
migration responsibility. Database behavior for wallet and quota data belongs
to `crates/aether-data` and `crates/aether-data-contracts`.

## Core Rules

Keep `aether-wallet` pure. Public methods should compute from their inputs and
return values. They should not read the clock, query storage, spawn tasks, make
network calls, or log side effects.

Keep money results quantized. Use `quantize_money` for derived wallet and quota
values:

```rust
// crates/aether-wallet/src/access.rs:128-130
pub fn quantize_money(value: f64) -> f64 {
    (value * 100_000_000.0).round() / 100_000_000.0
}
```

Keep string normalization centralized. Stored wallet and provider records use
strings in higher layers, but this crate turns them into enums through `parse`
helpers. Do not duplicate those comparisons in gateway or repository code.

Keep `None` meaningful. `WalletAccessDecision.remaining == None` means an
unlimited/admin allowance. `ProviderQuotaSnapshot.monthly_quota_usd == None`
means no monthly cap is known. Do not replace either with numeric sentinel
values.

## Quality Check

For any documentation-only change in this spec directory, run checks that prove
the spec is complete:

```bash
find .trellis/spec/aether-wallet/backend -maxdepth 1 -type f -name '*.md' -print | sort
for f in .trellis/spec/aether-wallet/backend/*.md; do wc -l "$f"; done
```

For any source change in `crates/aether-wallet`, run at least:

```bash
cargo test -p aether-wallet
```

If exported behavior changes, also run focused caller tests in
`apps/aether-gateway` or `crates/aether-scheduler-core` that map
`WalletAccessDecision`, `ProviderBillingType`, or `ProviderQuotaSnapshot`.

## Review Checklist

Reviewers should verify that:

1. `Cargo.toml` still has no runtime, database, logging, or error-handling
   dependency beyond the value-type need.
2. New public types are re-exported from `src/lib.rs`.
3. Parse helpers still normalize unknown values safely.
4. Wallet access ordering still handles admin, inactive status, negative
   recharge balance, unlimited mode, and finite balances in the documented
   order.
5. Quota reset logic remains deterministic with caller-provided timestamps.
6. Unit tests cover every added branch.
7. No code in this crate logs sensitive wallet identifiers, API key ids, user
   ids, balances, or provider quota internals.

## Language

All documentation in this directory is written in English.
