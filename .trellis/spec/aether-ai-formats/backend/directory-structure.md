# Directory Structure

`crates/aether-ai-formats` is a pure Rust foundation crate. Its directory structure is
organized by API surface and protocol responsibility, not by HTTP route, service, or
database entity. The important boundary is: caller-facing facade APIs at the top, stable
format contracts in the middle, and provider-specific parsing/encoding details in leaf
modules.

## Actual Layout

The ABCoder AST for `repo_name="aether-ai-formats"` lists 107 Rust source files. The
crate's physical source tree is organized like this:

```text
crates/aether-ai-formats/src/
|-- lib.rs
|-- api.rs
|-- contracts/
|   |-- actions.rs
|   |-- auth_context.rs
|   |-- control_payloads.rs
|   |-- plan_kinds.rs
|   `-- report_kinds.rs
|-- formats/
|   |-- claude/messages/{request,response,spec,stream,chat_spec,cli_spec}.rs
|   |-- gemini/{embedding,files,generate_content,video}/...
|   |-- openai/{chat,embedding,image,rerank,responses,video}/...
|   |-- doubao/embedding/request.rs
|   |-- jina/{embedding,rerank}/...
|   |-- shared/{error_body,request,response,routing,sse,stream_core,...}.rs
|   |-- context.rs
|   |-- id.rs
|   |-- matrix.rs
|   `-- registry.rs
|-- protocol/
|   |-- canonical.rs
|   `-- stream.rs
`-- provider_compat/
    |-- kiro_stream.rs
    |-- kiro_stream/...
    |-- private_envelope.rs
    |-- proxy/rules.rs
    `-- surfaces.rs
```

## Public Facades

Keep `src/lib.rs` narrow. It declares top-level modules and re-exports the stable, common
API that other crates can import without depending on internal module layout. Example:

```rust
// crates/aether-ai-formats/src/lib.rs:3-14
pub mod api;
pub mod contracts;
pub mod formats;
pub mod protocol;
pub mod provider_compat;

pub use formats::context::{FormatContext, FormatError};
pub use formats::id::{
    api_format_alias_matches, api_format_storage_aliases, api_format_uses_body_stream_field,
    is_openai_responses_compact_format, is_openai_responses_family_format,
    is_openai_responses_format, normalize_api_format_alias, FormatFamily, FormatId, FormatProfile,
};
```

Use `src/api.rs` when application crates need a broad compatibility surface. It intentionally
re-exports many symbols from contracts, formats, protocol, and provider compatibility in
one file. Example:

```rust
// crates/aether-ai-formats/src/api.rs:41-48
pub use crate::formats::claude::messages::stream::{ClaudeClientEmitter, ClaudeProviderState};
pub use crate::formats::gemini::generate_content::stream::{
    GeminiClientEmitter, GeminiProviderState,
};
pub use crate::formats::openai::chat::stream::{
    OpenAIChatClientEmitter, OpenAIChatProviderState, OpenAIResponsesClientEmitter,
    OpenAIResponsesProviderState,
};
```

Do not add low-level helpers to `api.rs` until they are actually needed by callers. Prefer
module-local private helpers for parsing details.

## Module Placement Rules

- Put wire-format identity, aliases, and provider families in `formats/id.rs`.
- Put cross-format request and response selection in `formats/matrix.rs`.
- Put top-level parse/emit/convert dispatch in `formats/registry.rs`.
- Put provider-specific sync request and response mapping under
  `formats/<provider>/<surface>/request.rs` and `response.rs`.
- Put provider-specific stream parsing/emitting under the same provider surface in
  `stream.rs`.
- Put canonical provider-neutral structs in `protocol/canonical.rs`; do not duplicate
  canonical request/response structs in provider modules.
- Put shared functions that are truly format-agnostic under `formats/shared/`.
- Put provider-private envelope and compatibility bridge code under `provider_compat/`,
  not under `formats/<provider>/`, unless the code maps a normal public provider API.

The module declarations reflect these boundaries:

```rust
// crates/aether-ai-formats/src/formats/mod.rs:1-11
pub mod claude;
pub mod context;
pub mod conversion;
pub mod doubao;
pub mod gemini;
pub mod id;
pub mod jina;
pub mod matrix;
pub mod openai;
pub mod registry;
pub mod shared;
```

Provider module names are lowercase, and API surfaces use snake_case directory names:
`generate_content`, `openai/responses`, `provider_compat/private_envelope`, and
`provider_compat/proxy`.

## Contracts Directory

`contracts/` is for execution-runtime vocabulary that must stay stable across crates.
Keep string constants and serialization payloads here rather than scattering literals in
gateway code. Example:

```rust
// crates/aether-ai-formats/src/contracts/mod.rs:7-12
pub use actions::{
    EXECUTION_RUNTIME_STREAM_ACTION, EXECUTION_RUNTIME_STREAM_DECISION_ACTION,
    EXECUTION_RUNTIME_SYNC_ACTION, EXECUTION_RUNTIME_SYNC_DECISION_ACTION,
};
pub use auth_context::ExecutionRuntimeAuthContext;
pub use control_payloads::{build_ai_control_plan_request, AiControlPlanRequest};
```

The control payload builder preserves the exact request shape:

```rust
// crates/aether-ai-formats/src/contracts/control_payloads.rs:20-40
#[allow(clippy::too_many_arguments)]
pub fn build_ai_control_plan_request(
    trace_id: &str,
    method: &str,
    path: &str,
    query_string: Option<&str>,
    headers: BTreeMap<String, String>,
    body_json: serde_json::Value,
    body_base64: Option<String>,
    auth_context: Option<ExecutionRuntimeAuthContext>,
) -> AiControlPlanRequest { ... }
```

## Provider-Specific Pattern

For standard text/chat formats, each provider surface usually has request, response, spec,
and stream modules. For example:

- `formats/openai/chat/request.rs`
- `formats/openai/chat/response.rs`
- `formats/openai/chat/stream.rs`
- `formats/claude/messages/request.rs`
- `formats/gemini/generate_content/stream.rs`

Leaf helpers should stay private unless another module needs them. In OpenAI image support,
the public API is limited to operations and normalized request functions while parsing
details stay private:

```rust
// crates/aether-ai-formats/src/formats/openai/image/request.rs:10-18
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAiImageOperation {
    Generate,
    Edit,
    Variation,
}

impl OpenAiImageOperation {
    pub fn as_str(self) -> &'static str { ... }
}
```

## Naming Conventions

- File and module names are snake_case.
- Provider identifiers use exact canonical strings such as `openai:chat`,
  `openai:responses:compact`, `claude:messages`, and `gemini:generate_content`.
- Format identity types are named `FormatId`, `FormatFamily`, and `FormatProfile`.
- Canonical data types use a `Canonical` prefix: `CanonicalRequest`,
  `CanonicalResponse`, `CanonicalContentBlock`, `CanonicalStreamFrame`.
- Stream state machines use explicit state names such as `OpenAIChatProviderState`,
  `GeminiClientEmitter`, `KiroToClaudeCliStreamState`, and `StreamingStandardFormatMatrix`.

## Do Not

Do not create route-style or service-style directories in this crate. There are no axum
handlers here. Do not add `db`, `repository`, `models`, or `migrations` directories.
Database, Redis, HTTP client, and tracing concerns belong in caller crates.

Do not add a new provider by changing only `api.rs`. A real provider surface must have
the correct placement across identity, matrix, registry, provider module, canonical
conversion, stream handling if applicable, and tests.
