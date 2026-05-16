# Security and Operations Notes

## Summary

`grok2api` is useful as a protocol reference, but its security posture should not be copied directly into Aether. Treat its account/session fields and proxy behavior as sensitive upstream-control material, and keep Aether's encrypted provider key contract as the required storage boundary.

## High-Risk Findings

### Default Admin Credential

`grok2api` ships with a default `app.app_key` and uses that shared token for `/admin/*` access. If an operator keeps defaults, the admin surface can expose account, proxy, and runtime config material.

References:

* `/Volumes/mayrain/workspace/private/grok2api/config.defaults.toml`
* `/Volumes/mayrain/workspace/private/grok2api/app/platform/auth/middleware.py`

Guardrail for Aether:

* Do not import or mirror `grok2api` admin auth.
* Require Aether management scopes/sessions for admin actions.
* Never expose raw secret-bearing config over admin read APIs.

### Remote Asset Fetch Can Leak Account Cookies

`grok2api` accepts remote image/file URLs in media workflows, and the upstream fetch path can combine user-controlled URLs with Grok session headers/cookies. That is an SSRF and credential-exfiltration risk if copied.

References:

* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/reverse/transport/asset_upload.py`
* `/Volumes/mayrain/workspace/private/grok2api/app/dataplane/proxy/adapters/headers.py`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/images.py`
* `/Volumes/mayrain/workspace/private/grok2api/app/products/openai/video.py`

Guardrail for Aether:

* Do not copy media upload/fetch/cache behavior into Aether.
* Reject remote file URLs by default.
* If remote fetch is later allowed, strip account auth headers and enforce host allowlists.

### Plaintext Token and Clearance Persistence

`grok2api` local/account config stores token-like material directly in its account/config stores. Aether already has an encrypted provider key auth config path, so Grok account material must go through that path instead.

References:

* `/Volumes/mayrain/workspace/private/grok2api/app/control/account/models.py`
* `/Volumes/mayrain/workspace/private/grok2api/app/control/account/backends/local.py`
* `/Volumes/mayrain/workspace/private/grok2api/app/platform/config/backends/toml.py`
* `/Volumes/mayrain/workspace/Aether/crates/aether-data-contracts/src/repository/provider_catalog/types.rs`
* `/Volumes/mayrain/workspace/Aether/crates/aether-provider-transport/src/snapshot_mapping.rs`

Guardrail for Aether:

* Store Grok session material only in encrypted `auth_config`.
* Keep stable non-secret fingerprints for duplicate detection.
* Redact session/cookie/proxy values in logs, admin payloads, and test snapshots.

### Fail-Open Public API Defaults

`grok2api` allows `/v1/*` without extra auth when `app.api_key` is empty. Aether should not inherit this behavior for integrated Grok access.

References:

* `/Volumes/mayrain/workspace/private/grok2api/README.md`
* `/Volumes/mayrain/workspace/private/grok2api/app/platform/auth/middleware.py`

Guardrail for Aether:

* Keep Aether's existing API-key and management-token model.
* Do not expose a public Grok passthrough endpoint without Aether auth and rate controls.

## Operational Requirements Before Implementation

* Define exactly which Grok account material Aether will store per provider key.
* Define refresh semantics: no refresh, imported session blob only, or native refresh adapter.
* Decide how proxy nodes are bound: imported arbitrary proxy URLs should be rejected; use Aether-approved proxy node IDs if needed.
* Add log redaction tests before any Grok account import path.
* Add admin-payload tests proving secrets are absent from pool/key responses.
* Add failure mapping for invalid credentials, forbidden/Cloudflare challenge, quota exhaustion, upstream 5xx, and transient proxy failure.

## MVP Security Boundary

For the first implementation pass:

* Chat/responses and image generation only.
* Aether owns auth, provider keys, pool selection, and admin visibility.
* Grok account/session material is encrypted and never returned raw.
* Media URL upload/fetch/cache, video jobs, WebUI/Admin mirroring, and `grok2api` local account DB import are out of scope.
