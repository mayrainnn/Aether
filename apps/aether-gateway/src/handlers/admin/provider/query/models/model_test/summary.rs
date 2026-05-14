use super::super::provider_query_key_display_name;
use super::{ProviderQueryExecutionOutcome, ProviderQueryTestCandidate};
use serde_json::{json, Value};

pub(super) fn provider_query_test_attempt_payload(
    candidate_index: usize,
    candidate: &ProviderQueryTestCandidate,
    execution: &ProviderQueryExecutionOutcome,
) -> Value {
    json!({
        "candidate_index": candidate_index,
        "retry_index": 0,
        "endpoint_api_format": candidate.endpoint.api_format,
        "endpoint_base_url": candidate.endpoint.base_url,
        "key_name": provider_query_key_display_name(&candidate.key),
        "key_id": candidate.key.id,
        "auth_type": candidate.key.auth_type,
        "effective_model": candidate.effective_model,
        "status": execution.status,
        "skip_reason": execution.skip_reason,
        "error_message": execution.error_message,
        "status_code": execution.status_code,
        "latency_ms": execution.latency_ms,
        "request_url": execution.request_url,
        "request_headers": execution.request_headers,
        "request_body": execution.request_body,
        "response_headers": execution.response_headers,
        "response_body": execution.response_body,
    })
}

pub(super) fn provider_query_candidate_summary_payload(
    total_candidates: usize,
    total_attempts: usize,
    attempts: &[Value],
) -> Value {
    let success_count = attempts
        .iter()
        .filter(|attempt| attempt.get("status").and_then(Value::as_str) == Some("success"))
        .count();
    let failed_count = attempts
        .iter()
        .filter(|attempt| {
            matches!(
                attempt.get("status").and_then(Value::as_str),
                Some("failed") | Some("cancelled") | Some("stream_interrupted")
            )
        })
        .count();
    let skipped_count = attempts
        .iter()
        .filter(|attempt| attempt.get("status").and_then(Value::as_str) == Some("skipped"))
        .count();
    let pending_count = attempts
        .iter()
        .filter(|attempt| {
            matches!(
                attempt.get("status").and_then(Value::as_str),
                Some("pending") | Some("streaming")
            )
        })
        .count();
    let available_count = attempts
        .iter()
        .filter(|attempt| attempt.get("status").and_then(Value::as_str) == Some("available"))
        .count();
    let unused_count = if success_count > 0 {
        total_candidates.saturating_sub(success_count + failed_count + skipped_count)
    } else {
        0
    };
    let stop_reason = if total_candidates == 0 {
        "no_candidate"
    } else if success_count > 0 {
        "first_success"
    } else if total_attempts == 0 && skipped_count > 0 {
        "all_skipped"
    } else if failed_count > 0 || skipped_count > 0 {
        "exhausted"
    } else {
        "pending"
    };
    let winning_attempt = attempts
        .iter()
        .find(|attempt| attempt.get("status").and_then(Value::as_str) == Some("success"));

    json!({
        "total_candidates": total_candidates,
        "attempted": total_attempts,
        "success": success_count,
        "failed": failed_count,
        "skipped": skipped_count,
        "unused": unused_count,
        "pending": pending_count,
        "available": available_count,
        "completed": success_count + failed_count + skipped_count + unused_count,
        "stop_reason": stop_reason,
        "winning_candidate_index": winning_attempt
            .and_then(|attempt| attempt.get("candidate_index"))
            .cloned()
            .unwrap_or(Value::Null),
        "winning_key_name": winning_attempt
            .and_then(|attempt| attempt.get("key_name"))
            .cloned()
            .unwrap_or(Value::Null),
        "winning_key_id": winning_attempt
            .and_then(|attempt| attempt.get("key_id"))
            .cloned()
            .unwrap_or(Value::Null),
        "winning_auth_type": winning_attempt
            .and_then(|attempt| attempt.get("auth_type"))
            .cloned()
            .unwrap_or(Value::Null),
        "winning_effective_model": winning_attempt
            .and_then(|attempt| attempt.get("effective_model"))
            .cloned()
            .unwrap_or(Value::Null),
        "winning_endpoint_api_format": winning_attempt
            .and_then(|attempt| attempt.get("endpoint_api_format"))
            .cloned()
            .unwrap_or(Value::Null),
        "winning_endpoint_base_url": winning_attempt
            .and_then(|attempt| attempt.get("endpoint_base_url"))
            .cloned()
            .unwrap_or(Value::Null),
        "winning_latency_ms": winning_attempt
            .and_then(|attempt| attempt.get("latency_ms"))
            .cloned()
            .unwrap_or(Value::Null),
        "winning_status_code": winning_attempt
            .and_then(|attempt| attempt.get("status_code"))
            .cloned()
            .unwrap_or(Value::Null),
    })
}
