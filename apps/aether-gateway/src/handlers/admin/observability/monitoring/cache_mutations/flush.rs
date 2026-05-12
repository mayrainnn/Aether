use super::super::cache_affinity::{
    clear_admin_monitoring_scheduler_affinity_entries,
    delete_admin_monitoring_cache_affinity_raw_keys,
};
use super::super::cache_store::list_admin_monitoring_cache_affinity_records;
use crate::handlers::admin::request::AdminAppState;
use crate::GatewayError;
use aether_admin::observability::monitoring::build_admin_monitoring_cache_flush_success_response;
use axum::{body::Body, response::Response};

pub(in super::super) async fn build_admin_monitoring_cache_flush_response(
    state: &AdminAppState<'_>,
) -> Result<Response<Body>, GatewayError> {
    let raw_affinities = list_admin_monitoring_cache_affinity_records(state).await?;
    let raw_keys = raw_affinities
        .iter()
        .map(|item| item.raw_key.clone())
        .collect::<Vec<_>>();
    state.as_ref().invalidate_scheduler_affinity_cache();
    let deleted = delete_admin_monitoring_cache_affinity_raw_keys(state, &raw_keys).await?;
    clear_admin_monitoring_scheduler_affinity_entries(state, &raw_affinities);

    Ok(build_admin_monitoring_cache_flush_success_response(deleted))
}
