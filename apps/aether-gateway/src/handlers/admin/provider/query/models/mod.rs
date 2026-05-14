use super::payload::{
    provider_query_extract_api_key_id, provider_query_extract_force_refresh,
    provider_query_extract_model, provider_query_extract_provider_id,
    provider_query_extract_request_id,
};
use super::response::{
    build_admin_provider_query_bad_request_response, build_admin_provider_query_not_found_response,
    ADMIN_PROVIDER_QUERY_API_KEY_NOT_FOUND_DETAIL, ADMIN_PROVIDER_QUERY_MODEL_REQUIRED_DETAIL,
    ADMIN_PROVIDER_QUERY_NO_ACTIVE_API_KEY_DETAIL, ADMIN_PROVIDER_QUERY_NO_LOCAL_MODELS_DETAIL,
    ADMIN_PROVIDER_QUERY_PROVIDER_ID_REQUIRED_DETAIL,
    ADMIN_PROVIDER_QUERY_PROVIDER_NOT_FOUND_DETAIL,
};
use crate::ai_serving::{
    maybe_build_sync_finalize_outcome, GatewayControlDecision,
    ANTIGRAVITY_V1INTERNAL_ENVELOPE_NAME, GEMINI_CHAT_SYNC_FINALIZE_REPORT_KIND,
    OPENAI_IMAGE_SYNC_FINALIZE_REPORT_KIND,
};
use crate::clock::current_unix_ms;
use crate::execution_runtime;
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use crate::handlers::shared::provider_pool::{
    admin_provider_pool_config_from_config_value, read_admin_provider_pool_runtime_state,
    AdminProviderPoolConfig, AdminProviderPoolRuntimeState,
};
use crate::handlers::shared::{
    parse_catalog_auth_config_json, provider_key_health_summary,
    provider_key_status_snapshot_payload,
};
use crate::model_fetch::ModelFetchRuntimeState;
use crate::provider_key_auth::{
    provider_key_auth_semantics, provider_key_configured_api_formats,
    provider_key_inherits_provider_api_formats,
};
use crate::provider_transport::antigravity::{
    build_antigravity_safe_v1internal_request, build_antigravity_static_identity_headers,
    classify_local_antigravity_request_support, AntigravityEnvelopeRequestType,
    AntigravityRequestEnvelopeSupport, AntigravityRequestSideSupport,
    AntigravityRequestSideUnsupportedReason,
};
use crate::provider_transport::kiro::{
    build_kiro_generate_assistant_response_url, build_kiro_provider_headers,
    build_kiro_provider_request_body, supports_local_kiro_request_transport_with_network,
    KiroProviderHeadersInput, KIRO_ENVELOPE_NAME,
};
use crate::usage::GatewaySyncReportRequest;
use crate::{AppState, GatewayError};
use aether_admin::provider::pool as admin_provider_pool_pure;
use aether_ai_serving::{
    run_ai_pool_scheduler, AiPoolCandidateFacts, AiPoolCandidateInput, AiPoolCatalogKeyContext,
    AiPoolRuntimeState, AiPoolSchedulingConfig, AiPoolSchedulingPreset,
};
use aether_contracts::{ExecutionPlan, RequestBody};
use aether_data_contracts::repository::candidate_selection::{
    StoredMinimalCandidateSelectionRow, StoredProviderModelMapping,
};
use aether_data_contracts::repository::global_models::{
    AdminProviderModelListQuery, StoredAdminProviderModel,
};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_model_fetch::{
    aggregate_models_for_cache, fetch_models_from_transports, json_string_list,
    preset_models_for_provider, selected_models_fetch_endpoints,
};
use axum::{
    body::{to_bytes, Body},
    http::{self, HeaderMap, HeaderName, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine as _;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use uuid::Uuid;

pub(crate) const ADMIN_PROVIDER_QUERY_LOCAL_TEST_MODEL_MESSAGE: &str =
    "Rust local provider-query model test is not configured";
pub(crate) const ADMIN_PROVIDER_QUERY_LOCAL_TEST_MODEL_FAILOVER_MESSAGE: &str =
    "Rust local provider-query failover simulation is not configured";
const ADMIN_PROVIDER_QUERY_NO_ACTIVE_ENDPOINT_DETAIL: &str =
    "No active endpoints found for this provider";
const ADMIN_PROVIDER_QUERY_NO_MODELS_FROM_ENDPOINT_DETAIL: &str =
    "No models returned from any endpoint";
const ADMIN_PROVIDER_QUERY_NO_MODELS_FROM_KEY_DETAIL: &str = "No models returned from any key";
const ADMIN_PROVIDER_QUERY_NO_ACTIVE_TEST_CANDIDATE_DETAIL: &str =
    "No active endpoint or API key found";
const ADMIN_PROVIDER_QUERY_INVALID_MAPPED_MODEL_DETAIL: &str =
    "mapped_model_name is not valid for the selected model and endpoint";
const ANTIGRAVITY_PROVIDER_CACHE_KEY_PREFIX: &str = "upstream_models_provider:";
const DEFAULT_PROVIDER_QUERY_TEST_MESSAGE: &str = "Hello! This is a test message.";
static PROVIDER_QUERY_POOL_LOAD_BALANCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct ProviderQueryKeyFetchResult {
    models: Vec<Value>,
    error: Option<String>,
    from_cache: bool,
    has_success: bool,
}

fn provider_query_codex_preset_fallback(
    provider: &StoredProviderCatalogProvider,
) -> Option<ProviderQueryKeyFetchResult> {
    if !provider.provider_type.trim().eq_ignore_ascii_case("codex") {
        return None;
    }
    let models = preset_models_for_provider(&provider.provider_type)?;
    Some(ProviderQueryKeyFetchResult {
        models: aggregate_models_for_cache(&models),
        error: None,
        from_cache: false,
        has_success: true,
    })
}

mod model_test;

pub(crate) use self::model_test::{
    build_admin_provider_query_test_model_failover_local_response,
    build_admin_provider_query_test_model_failover_response,
    build_admin_provider_query_test_model_local_response,
    build_admin_provider_query_test_model_response,
};

fn provider_query_provider_payload(provider: &StoredProviderCatalogProvider) -> Value {
    json!({
        "id": provider.id.clone(),
        "name": provider.name.clone(),
        "display_name": provider.name.clone(),
        "provider_type": provider.provider_type.clone(),
    })
}

fn provider_query_key_display_name(key: &StoredProviderCatalogKey) -> String {
    let trimmed = key.name.trim();
    if trimmed.is_empty() {
        key.id.clone()
    } else {
        trimmed.to_string()
    }
}

async fn provider_query_read_cached_models(
    state: &AdminAppState<'_>,
    provider_id: &str,
    key_id: &str,
) -> Option<Vec<Value>> {
    let cache_key = format!("upstream_models:{provider_id}:{key_id}");
    let raw = state.runtime_state().kv_get(&cache_key).await.ok()??;
    let parsed = serde_json::from_str::<Vec<Value>>(&raw).ok()?;
    Some(aggregate_models_for_cache(&parsed))
}

async fn provider_query_read_provider_cached_models(
    state: &AdminAppState<'_>,
    provider_id: &str,
) -> Option<Vec<Value>> {
    let cache_key = format!("{ANTIGRAVITY_PROVIDER_CACHE_KEY_PREFIX}{provider_id}");
    let raw = state.runtime_state().kv_get(&cache_key).await.ok()??;
    let parsed = serde_json::from_str::<Vec<Value>>(&raw).ok()?;
    Some(aggregate_models_for_cache(&parsed))
}

async fn provider_query_write_provider_cached_models(
    state: &AdminAppState<'_>,
    provider_id: &str,
    models: &[Value],
) {
    let Ok(serialized) = serde_json::to_string(&aggregate_models_for_cache(models)) else {
        return;
    };
    let cache_key = format!("{ANTIGRAVITY_PROVIDER_CACHE_KEY_PREFIX}{provider_id}");
    let _ = state
        .runtime_state()
        .kv_set(
            &cache_key,
            serialized,
            Some(std::time::Duration::from_secs(
                aether_model_fetch::model_fetch_interval_minutes().saturating_mul(60),
            )),
        )
        .await;
}

fn provider_query_antigravity_tier_weight(raw_auth_config: Option<&str>) -> i32 {
    raw_auth_config
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .and_then(|value| value.get("tier").cloned())
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .map(|tier| match tier.trim().to_ascii_lowercase().as_str() {
            "ultra" => 3,
            "pro" => 2,
            "free" => 1,
            _ => 0,
        })
        .unwrap_or(0)
}

async fn provider_query_sort_antigravity_keys(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoints: &[StoredProviderCatalogEndpoint],
    keys: Vec<StoredProviderCatalogKey>,
) -> Result<Vec<StoredProviderCatalogKey>, GatewayError> {
    let mut ranked = Vec::new();
    for key in keys {
        let availability = if key.oauth_invalid_at_unix_secs.is_some() {
            0
        } else {
            1
        };
        let tier_weight = if let Some(endpoint) = selected_models_fetch_endpoints(endpoints, &key)
            .into_iter()
            .next()
        {
            state
                .app()
                .read_provider_transport_snapshot(&provider.id, &endpoint.id, &key.id)
                .await?
                .map(|transport| {
                    provider_query_antigravity_tier_weight(
                        transport.key.decrypted_auth_config.as_deref(),
                    )
                })
                .unwrap_or(0)
        } else {
            0
        };
        ranked.push(((availability, tier_weight), key));
    }
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    Ok(ranked.into_iter().map(|(_, key)| key).collect())
}

async fn provider_query_fetch_models_for_key(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoints: &[StoredProviderCatalogEndpoint],
    key: &StoredProviderCatalogKey,
    force_refresh: bool,
) -> Result<ProviderQueryKeyFetchResult, GatewayError> {
    if !force_refresh {
        if let Some(cached_models) =
            provider_query_read_cached_models(state, &provider.id, &key.id).await
        {
            return Ok(ProviderQueryKeyFetchResult {
                models: cached_models,
                error: None,
                from_cache: true,
                has_success: true,
            });
        }
    }

    let selected_endpoints = selected_models_fetch_endpoints(endpoints, key);
    if selected_endpoints.is_empty() {
        if let Some(models) = preset_models_for_provider(&provider.provider_type) {
            return Ok(ProviderQueryKeyFetchResult {
                models: aggregate_models_for_cache(&models),
                error: None,
                from_cache: false,
                has_success: true,
            });
        }
        return Ok(ProviderQueryKeyFetchResult {
            models: Vec::new(),
            error: Some(ADMIN_PROVIDER_QUERY_NO_ACTIVE_ENDPOINT_DETAIL.to_string()),
            from_cache: false,
            has_success: false,
        });
    }

    let mut transports = Vec::new();
    let mut all_errors = Vec::new();
    for endpoint in selected_endpoints {
        let Some(transport) = state
            .app()
            .read_provider_transport_snapshot(&provider.id, &endpoint.id, &key.id)
            .await?
        else {
            all_errors.push(format!(
                "{} transport snapshot unavailable",
                endpoint.api_format.trim()
            ));
            continue;
        };
        transports.push(transport);
    }

    if transports.is_empty() {
        return Ok(ProviderQueryKeyFetchResult {
            models: Vec::new(),
            error: Some(all_errors.join("; ")),
            from_cache: false,
            has_success: false,
        });
    }

    let outcome = match fetch_models_from_transports(state.app(), &transports).await {
        Ok(outcome) => outcome,
        Err(err) => {
            all_errors.push(err);
            if let Some(fallback) = provider_query_codex_preset_fallback(provider) {
                return Ok(fallback);
            }
            return Ok(ProviderQueryKeyFetchResult {
                models: Vec::new(),
                error: Some(all_errors.join("; ")),
                from_cache: false,
                has_success: false,
            });
        }
    };

    all_errors.extend(outcome.errors);
    let unique_models = aggregate_models_for_cache(&outcome.cached_models);
    if outcome.has_success && !unique_models.is_empty() {
        <AppState as ModelFetchRuntimeState>::write_upstream_models_cache(
            state.app(),
            &provider.id,
            &key.id,
            &unique_models,
        )
        .await;
    }

    if unique_models.is_empty() && !all_errors.is_empty() {
        if let Some(fallback) = provider_query_codex_preset_fallback(provider) {
            return Ok(fallback);
        }
    }

    let mut error = if all_errors.is_empty() {
        None
    } else {
        Some(all_errors.join("; "))
    };
    if unique_models.is_empty() && error.is_none() {
        error = Some(ADMIN_PROVIDER_QUERY_NO_MODELS_FROM_ENDPOINT_DETAIL.to_string());
    }

    Ok(ProviderQueryKeyFetchResult {
        models: unique_models,
        error,
        from_cache: false,
        has_success: outcome.has_success,
    })
}

pub(crate) async fn build_admin_provider_query_models_response(
    state: &AdminAppState<'_>,
    payload: &serde_json::Value,
) -> Result<Response<Body>, GatewayError> {
    let Some(provider_id) = provider_query_extract_provider_id(payload) else {
        return Ok(build_admin_provider_query_bad_request_response(
            ADMIN_PROVIDER_QUERY_PROVIDER_ID_REQUIRED_DETAIL,
        ));
    };

    let Some(provider) = state
        .app()
        .read_provider_catalog_providers_by_ids(std::slice::from_ref(&provider_id))
        .await?
        .into_iter()
        .find(|item| item.id == provider_id)
    else {
        return Ok(build_admin_provider_query_not_found_response(
            ADMIN_PROVIDER_QUERY_PROVIDER_NOT_FOUND_DETAIL,
        ));
    };

    let provider_ids = vec![provider.id.clone()];
    let endpoints = state
        .app()
        .list_provider_catalog_endpoints_by_provider_ids(&provider_ids)
        .await?;
    let keys = state
        .app()
        .list_provider_catalog_keys_by_provider_ids(&provider_ids)
        .await?;
    let force_refresh = provider_query_extract_force_refresh(payload);

    if let Some(api_key_id) = provider_query_extract_api_key_id(payload) {
        let Some(selected_key) = keys.iter().find(|key| key.id == api_key_id) else {
            return Ok(build_admin_provider_query_not_found_response(
                ADMIN_PROVIDER_QUERY_API_KEY_NOT_FOUND_DETAIL,
            ));
        };

        let result = provider_query_fetch_models_for_key(
            state,
            &provider,
            &endpoints,
            selected_key,
            force_refresh,
        )
        .await?;
        let success = !result.models.is_empty();
        return Ok(Json(json!({
            "success": success,
            "data": {
                "models": result.models,
                "error": result.error,
                "from_cache": result.from_cache,
            },
            "provider": provider_query_provider_payload(&provider),
        }))
        .into_response());
    }

    let active_keys = keys
        .into_iter()
        .filter(|key| key.is_active)
        .collect::<Vec<_>>();
    if active_keys.is_empty() {
        return Ok(build_admin_provider_query_bad_request_response(
            ADMIN_PROVIDER_QUERY_NO_ACTIVE_API_KEY_DETAIL,
        ));
    }
    let active_key_count = active_keys.len();

    if provider
        .provider_type
        .trim()
        .eq_ignore_ascii_case("antigravity")
        && !force_refresh
    {
        if let Some(models) = provider_query_read_provider_cached_models(state, &provider.id).await
        {
            return Ok(Json(json!({
                "success": !models.is_empty(),
                "data": {
                    "models": models,
                    "error": serde_json::Value::Null,
                    "from_cache": true,
                    "keys_total": active_key_count,
                    "keys_cached": active_key_count,
                    "keys_fetched": 0,
                },
                "provider": provider_query_provider_payload(&provider),
            }))
            .into_response());
        }
    }

    let ordered_keys = if provider
        .provider_type
        .trim()
        .eq_ignore_ascii_case("antigravity")
    {
        provider_query_sort_antigravity_keys(state, &provider, &endpoints, active_keys).await?
    } else {
        active_keys
    };

    let mut all_models = Vec::new();
    let mut all_errors = Vec::new();
    let mut cache_hit_count = 0usize;
    let mut fetch_count = 0usize;
    for key in &ordered_keys {
        let result =
            provider_query_fetch_models_for_key(state, &provider, &endpoints, key, force_refresh)
                .await?;
        all_models.extend(result.models);
        if let Some(error) = result.error {
            all_errors.push(format!(
                "Key {}: {}",
                provider_query_key_display_name(key),
                error
            ));
        }
        if result.from_cache {
            cache_hit_count += 1;
        } else {
            fetch_count += 1;
        }
        if provider
            .provider_type
            .trim()
            .eq_ignore_ascii_case("antigravity")
            && result.has_success
        {
            break;
        }
    }

    let models = aggregate_models_for_cache(&all_models);
    if provider
        .provider_type
        .trim()
        .eq_ignore_ascii_case("antigravity")
        && !models.is_empty()
    {
        provider_query_write_provider_cached_models(state, &provider.id, &models).await;
    }
    let success = !models.is_empty();
    let mut error = if all_errors.is_empty() {
        None
    } else {
        Some(all_errors.join("; "))
    };
    if !success && error.is_none() {
        error = Some(ADMIN_PROVIDER_QUERY_NO_MODELS_FROM_KEY_DETAIL.to_string());
    }

    Ok(Json(json!({
        "success": success,
        "data": {
            "models": models,
            "error": error,
            "from_cache": fetch_count == 0 && cache_hit_count > 0,
            "keys_total": active_key_count,
            "keys_cached": cache_hit_count,
            "keys_fetched": fetch_count,
        },
        "provider": provider_query_provider_payload(&provider),
    }))
    .into_response())
}
