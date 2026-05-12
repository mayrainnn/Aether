#[derive(Debug, Clone)]
pub(super) struct AdminMonitoringCacheAffinityRecord {
    pub(super) raw_key: String,
    pub(super) affinity_key: String,
    pub(super) api_format: String,
    pub(super) model_name: String,
    pub(super) client_family: Option<String>,
    pub(super) session_hash: Option<String>,
    pub(super) provider_id: Option<String>,
    pub(super) endpoint_id: Option<String>,
    pub(super) key_id: Option<String>,
    pub(super) created_at: Option<serde_json::Value>,
    pub(super) expire_at: Option<serde_json::Value>,
    pub(super) request_count: u64,
    pub(super) request_count_known: bool,
    pub(super) scheduler_affinity_epoch: Option<u64>,
}

pub(super) struct AdminMonitoringCacheSnapshot {
    pub(super) scheduler_name: String,
    pub(super) scheduling_mode: String,
    pub(super) provider_priority_mode: String,
    pub(super) storage_type: &'static str,
    pub(super) total_affinities: usize,
    pub(super) cache_hits: usize,
    pub(super) cache_misses: usize,
    pub(super) cache_hit_rate: f64,
    pub(super) provider_switches: usize,
    pub(super) key_switches: usize,
    pub(super) cache_invalidations: usize,
}
