# Design Grok browser transport profile integration

## Goal

Represent Grok browser-sensitive upstream transport through Aether's existing
`fingerprint.transport_profile` contract, so the Grok PR does not introduce a
parallel fingerprint configuration system while still using `wreq` for real
browser-style HTTP/TLS/WebSocket behavior.

## User Value

The PR should be easier for upstream maintainers to accept because it extends
the existing transport profile abstraction instead of adding Grok-only
fingerprint semantics. Operators should have one place to inspect and override
browser transport behavior.

## Confirmed Facts

* Existing key and provider configs already support `fingerprint.transport_profile`.
* `resolve_transport_profile` currently resolves key fingerprint first, then
  provider default config, then the new Grok auth-config fallback.
* Existing Claude Code fingerprint generation is header-oriented and defaults
  to `reqwest_rustls`; it is not a browser ClientHello implementation.
* `browser_wreq` is already defined as a transport backend constant in
  `aether-contracts`.
* Gateway execution branches by `ResolvedTransportProfile.backend`, so
  `browser_wreq` can remain provider-agnostic execution capability.
* Grok currently stores `browser_profile` and `user_agent` in encrypted auth
  config during import; this risks becoming a second browser fingerprint truth.
* Grok Web requests need both browser-looking headers and a browser-like
  transport backend. Headers alone are insufficient.

## Requirements

* Use `fingerprint.transport_profile` as the preferred source of Grok browser
  transport configuration.
* Keep `wreq` as the execution backend for `browser_wreq`.
* Do not reuse Claude Code fingerprint generation directly for Grok.
* Preserve compatibility with already-imported Grok accounts that only have
  `browser_profile` / `user_agent` in auth config.
* Keep Grok browser defaults single-sourced so runtime, quota, headers, and
  diagnostics do not drift.
* Fail loudly for unsupported browser profiles instead of silently downgrading.
* Avoid adding provider-name checks in the generic execution runtime.

## Acceptance Criteria

* A Grok key with `key.fingerprint.transport_profile.backend = browser_wreq`
  resolves to that profile and does not consult Grok auth-config browser
  fallback.
* A Grok key without fingerprint but with legacy auth-config `browser_profile`
  still resolves to a compatible `browser_wreq` profile.
* A Grok key without fingerprint or browser profile receives one default Grok
  browser profile through provider-transport resolution.
* Grok request headers derive their UA / Client Hints from the same resolved
  browser profile used by `wreq`.
* Grok quota, chat, responses, image, and WebSocket paths do not each maintain
  separate browser constants.
* Diagnostics show both configured fingerprint transport profile and resolved
  transport profile for Grok.
* Targeted provider-transport and gateway tests cover preferred fingerprint,
  legacy fallback, default fallback, and unsupported profile behavior.

## Out Of Scope

* Removing `wreq`.
* Capturing real emitted JA3/JA4 from `wreq`.
* Building a generic browser profile UI in this task.
* Rewriting all provider fingerprint generation.
* Live Grok network integration tests.

## Open Question

Should new Grok account imports write the inferred browser profile into
`key.fingerprint.transport_profile` immediately, or should import keep writing
only auth config and let provider-transport synthesize the profile at runtime?

Recommended answer: write the inferred profile into `key.fingerprint` for new
imports, while preserving auth-config fallback for existing keys. This gives
operators an inspectable and overrideable transport truth without breaking
legacy imported accounts.
