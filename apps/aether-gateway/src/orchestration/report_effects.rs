use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use aether_admin::provider::quota as admin_provider_quota_pure;
use aether_usage_runtime::{
    extract_gemini_file_mapping_entries, gemini_file_mapping_cache_key, normalize_gemini_file_name,
    report_request_id, GatewayStreamReportRequest, GatewaySyncReportRequest,
    GEMINI_FILE_MAPPING_TTL_SECONDS,
};
use serde_json::Value;
use tracing::warn;
use uuid::Uuid;

use crate::clock::current_unix_secs;
use crate::handlers::shared::sync_provider_key_quota_status_snapshot;
use crate::log_ids::short_request_id;
use crate::{AppState, GatewayError};

const CODEX_QUOTA_CACHE_TTL_SECONDS: u64 = 30;
const CODEX_QUOTA_CACHE_MAX_ENTRIES: usize = 4096;

type HeaderFingerprintCache = Mutex<HashMap<String, (String, Instant)>>;

static CODEX_QUOTA_HEADER_FINGERPRINT_CACHE: OnceLock<HeaderFingerprintCache> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub(crate) enum LocalReportEffect<'a> {
    Sync {
        payload: &'a GatewaySyncReportRequest,
    },
    Stream {
        payload: &'a GatewayStreamReportRequest,
    },
}

pub(crate) async fn apply_local_report_effect(state: &AppState, effect: LocalReportEffect<'_>) {
    match effect {
        LocalReportEffect::Sync { payload } => {
            apply_local_sync_report_effect(state, payload).await;
        }
        LocalReportEffect::Stream { payload } => {
            apply_local_stream_report_effect(state, payload).await;
        }
    }
}

fn codex_quota_header_fingerprint_cache() -> &'static HeaderFingerprintCache {
    CODEX_QUOTA_HEADER_FINGERPRINT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn report_context_key_id(report_context: Option<&Value>) -> Option<String> {
    report_context
        .and_then(|context| context.get("key_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn report_context_provider_response_headers(
    report_context: Option<&Value>,
) -> Option<BTreeMap<String, String>> {
    let headers = report_context
        .and_then(|context| context.get("provider_response_headers"))
        .and_then(Value::as_object)?;
    let mut out = BTreeMap::new();
    for (key, value) in headers {
        let Some(value) = value.as_str() else {
            continue;
        };
        out.insert(key.clone(), value.to_string());
    }
    (!out.is_empty()).then_some(out)
}

fn is_volatile_compare_field(key: &str) -> bool {
    key == "updated_at" || key.ends_with("_reset_seconds") || key.ends_with("_reset_after_seconds")
}

fn canonicalize_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_value).collect()),
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            let mut normalized = serde_json::Map::new();
            for (key, value) in entries {
                normalized.insert(key.clone(), canonicalize_value(value));
            }
            Value::Object(normalized)
        }
        _ => value.clone(),
    }
}

fn fingerprint_codex_payload(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    let mut entries = object
        .iter()
        .filter(|(key, _)| !is_volatile_compare_field(key))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));

    let mut normalized = serde_json::Map::new();
    for (key, value) in entries {
        normalized.insert(key.clone(), canonicalize_value(value));
    }
    serde_json::to_string(&Value::Object(normalized)).ok()
}

fn get_cached_codex_quota_fingerprint(key_id: &str, now: Instant) -> Option<String> {
    let mut cache = codex_quota_header_fingerprint_cache()
        .lock()
        .expect("codex realtime quota cache should lock");
    match cache.get(key_id) {
        Some((fingerprint, expires_at)) if *expires_at > now => Some(fingerprint.clone()),
        Some(_) => {
            cache.remove(key_id);
            None
        }
        None => None,
    }
}

fn set_cached_codex_quota_fingerprint(key_id: &str, fingerprint: String, now: Instant) {
    let mut cache = codex_quota_header_fingerprint_cache()
        .lock()
        .expect("codex realtime quota cache should lock");
    cache.insert(
        key_id.to_string(),
        (
            fingerprint,
            now.checked_add(Duration::from_secs(CODEX_QUOTA_CACHE_TTL_SECONDS))
                .unwrap_or(now),
        ),
    );

    cache.retain(|_, (_, expires_at)| *expires_at > now);
    if cache.len() <= CODEX_QUOTA_CACHE_MAX_ENTRIES {
        return;
    }

    let mut entries = cache
        .iter()
        .map(|(key, (_, expires_at))| (key.clone(), *expires_at))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.1);
    for (key, _) in entries
        .into_iter()
        .take(cache.len() - CODEX_QUOTA_CACHE_MAX_ENTRIES)
    {
        cache.remove(&key);
    }
}

fn merge_metadata_object(
    current: Option<&Value>,
    section_key: &str,
    section_value: Value,
) -> Option<Value> {
    let mut merged = current
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    merged.insert(section_key.to_string(), section_value);
    Some(Value::Object(merged))
}

async fn apply_local_sync_report_effect(state: &AppState, payload: &GatewaySyncReportRequest) {
    apply_local_gemini_file_mapping_report_effect(state, payload).await;
    if let Err(err) = sync_codex_quota_from_response_headers(
        state,
        payload.report_context.as_ref(),
        &payload.headers,
    )
    .await
    {
        warn!(
            event_name = "codex_realtime_quota_sync_failed",
            log_type = "ops",
            report_kind = %payload.report_kind,
            report_request_id = %short_request_id(report_request_id(payload.report_context.as_ref())),
            error = ?err,
            "gateway failed to persist codex realtime quota from sync response headers"
        );
    }
}

async fn apply_local_stream_report_effect(state: &AppState, payload: &GatewayStreamReportRequest) {
    if let Err(err) = sync_codex_quota_from_response_headers(
        state,
        payload.report_context.as_ref(),
        &payload.headers,
    )
    .await
    {
        warn!(
            event_name = "codex_realtime_quota_sync_failed",
            log_type = "ops",
            report_kind = %payload.report_kind,
            report_request_id = %short_request_id(report_request_id(payload.report_context.as_ref())),
            error = ?err,
            "gateway failed to persist codex realtime quota from stream response headers"
        );
    }
}

async fn apply_local_gemini_file_mapping_report_effect(
    state: &AppState,
    payload: &GatewaySyncReportRequest,
) {
    match payload.report_kind.as_str() {
        "gemini_files_store_mapping" => {
            if payload.status_code >= 300 {
                return;
            }

            let key_id = payload
                .report_context
                .as_ref()
                .and_then(|context| context.get("file_key_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let user_id = payload
                .report_context
                .as_ref()
                .and_then(|context| context.get("user_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(key_id) = key_id else {
                return;
            };

            for entry in extract_gemini_file_mapping_entries(payload) {
                if let Err(err) = store_local_gemini_file_mapping(
                    state,
                    entry.file_name.as_str(),
                    key_id,
                    user_id,
                    entry.display_name.as_deref(),
                    entry.mime_type.as_deref(),
                )
                .await
                {
                    warn!(
                        event_name = "gemini_file_mapping_store_failed",
                        log_type = "ops",
                        report_kind = %payload.report_kind,
                        report_request_id = %short_request_id(report_request_id(payload.report_context.as_ref())),
                        file_name = %entry.file_name,
                        error = ?err,
                        "gateway failed to persist gemini file mapping locally"
                    );
                }
            }
        }
        "gemini_files_delete_mapping" if payload.status_code < 300 => {
            let file_name = payload
                .report_context
                .as_ref()
                .and_then(|context| context.get("file_name"))
                .and_then(Value::as_str)
                .and_then(normalize_gemini_file_name);
            let Some(file_name) = file_name else {
                return;
            };

            if let Err(err) = delete_local_gemini_file_mapping(state, file_name.as_str()).await {
                warn!(
                    event_name = "gemini_file_mapping_delete_failed",
                    log_type = "ops",
                    report_kind = %payload.report_kind,
                    report_request_id = %short_request_id(report_request_id(payload.report_context.as_ref())),
                    file_name = %file_name,
                    error = ?err,
                    "gateway failed to delete gemini file mapping locally"
                );
            }
        }
        _ => {}
    }
}

pub(crate) async fn store_local_gemini_file_mapping(
    state: &AppState,
    file_name: &str,
    key_id: &str,
    user_id: Option<&str>,
    display_name: Option<&str>,
    mime_type: Option<&str>,
) -> Result<(), GatewayError> {
    let Some(file_name) = normalize_gemini_file_name(file_name) else {
        return Ok(());
    };
    let expires_at_unix_secs = current_unix_secs().saturating_add(GEMINI_FILE_MAPPING_TTL_SECONDS);

    let _stored = state
        .upsert_gemini_file_mapping(
            aether_data::repository::gemini_file_mappings::UpsertGeminiFileMappingRecord {
                id: Uuid::new_v4().to_string(),
                file_name: file_name.clone(),
                key_id: key_id.to_string(),
                user_id: user_id.map(ToOwned::to_owned),
                display_name: display_name.map(ToOwned::to_owned),
                mime_type: mime_type.map(ToOwned::to_owned),
                source_hash: None,
                expires_at_unix_secs,
            },
        )
        .await?;
    state
        .cache_set_string_with_ttl(
            gemini_file_mapping_cache_key(file_name.as_str()).as_str(),
            key_id,
            GEMINI_FILE_MAPPING_TTL_SECONDS,
        )
        .await?;
    Ok(())
}

async fn delete_local_gemini_file_mapping(
    state: &AppState,
    file_name: &str,
) -> Result<(), GatewayError> {
    let Some(file_name) = normalize_gemini_file_name(file_name) else {
        return Ok(());
    };

    let _deleted = state
        .delete_gemini_file_mapping_by_file_name(file_name.as_str())
        .await?;
    state
        .cache_delete_key(gemini_file_mapping_cache_key(file_name.as_str()).as_str())
        .await?;
    Ok(())
}

async fn sync_codex_quota_from_response_headers(
    state: &AppState,
    report_context: Option<&Value>,
    headers: &BTreeMap<String, String>,
) -> Result<bool, GatewayError> {
    let key_id = match report_context_key_id(report_context) {
        Some(value) => value,
        None => return Ok(false),
    };

    let now_unix_secs = current_unix_secs();
    let provider_headers = report_context_provider_response_headers(report_context);
    let parsed_from_provider_headers = provider_headers.as_ref().and_then(|headers| {
        admin_provider_quota_pure::parse_codex_usage_headers(headers, now_unix_secs)
    });
    let Some(parsed) = parsed_from_provider_headers
        .or_else(|| admin_provider_quota_pure::parse_codex_usage_headers(headers, now_unix_secs))
    else {
        return Ok(false);
    };
    let Some(incoming_fingerprint) = fingerprint_codex_payload(&parsed) else {
        return Ok(false);
    };

    let now = Instant::now();
    if get_cached_codex_quota_fingerprint(&key_id, now).as_deref()
        == Some(incoming_fingerprint.as_str())
    {
        return Ok(false);
    }

    let Some(key) = state
        .read_provider_catalog_keys_by_ids(std::slice::from_ref(&key_id))
        .await?
        .into_iter()
        .next()
    else {
        set_cached_codex_quota_fingerprint(&key_id, incoming_fingerprint, now);
        return Ok(false);
    };

    let Some(provider) = state
        .read_provider_catalog_providers_by_ids(std::slice::from_ref(&key.provider_id))
        .await?
        .into_iter()
        .next()
    else {
        set_cached_codex_quota_fingerprint(&key_id, incoming_fingerprint, now);
        return Ok(false);
    };
    if !provider.provider_type.trim().eq_ignore_ascii_case("codex") {
        set_cached_codex_quota_fingerprint(&key_id, incoming_fingerprint, now);
        return Ok(false);
    }

    let current_codex = key
        .upstream_metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("codex"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(serde_json::Map::new);
    let current_codex = Value::Object(current_codex);
    let Some(current_fingerprint) = fingerprint_codex_payload(&current_codex) else {
        set_cached_codex_quota_fingerprint(&key_id, incoming_fingerprint, now);
        return Ok(false);
    };
    if current_fingerprint == incoming_fingerprint {
        set_cached_codex_quota_fingerprint(&key_id, incoming_fingerprint, now);
        return Ok(false);
    }

    let updated_upstream_metadata =
        merge_metadata_object(key.upstream_metadata.as_ref(), "codex", parsed);
    let updated_status_snapshot = sync_provider_key_quota_status_snapshot(
        key.status_snapshot.as_ref(),
        provider.provider_type.as_str(),
        updated_upstream_metadata.as_ref(),
        "response_headers",
    );
    let mut updated_key = key;
    updated_key.upstream_metadata = updated_upstream_metadata;
    updated_key.status_snapshot = updated_status_snapshot;
    updated_key.updated_at_unix_secs = Some(now_unix_secs);

    let updated = state
        .update_provider_catalog_key(&updated_key)
        .await?
        .is_some();
    if updated {
        set_cached_codex_quota_fingerprint(&key_id, incoming_fingerprint, now);
    }
    Ok(updated)
}

#[cfg(test)]
pub(crate) fn clear_local_report_effect_caches_for_tests() {
    if let Some(cache) = CODEX_QUOTA_HEADER_FINGERPRINT_CACHE.get() {
        cache
            .lock()
            .expect("codex realtime quota cache should lock")
            .clear();
    }
}
