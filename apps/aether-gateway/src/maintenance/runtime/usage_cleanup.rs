use aether_data_contracts::repository::usage::{UsageCleanupSummary, UsageCleanupWindow};
use aether_data_contracts::DataLayerError;
use chrono::Utc;

use crate::data::GatewayDataState;

use super::{
    system_config_bool, usage_cleanup_settings, usage_cleanup_window,
    usage_cleanup_window_with_override,
};

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub(crate) struct ManualUsageCleanupPreview {
    pub detail_cutoff: chrono::DateTime<Utc>,
    pub compressed_cutoff: chrono::DateTime<Utc>,
    pub header_cutoff: chrono::DateTime<Utc>,
    pub log_cutoff: chrono::DateTime<Utc>,
    pub requested_older_than_days: Option<u32>,
    pub detail_count: u64,
    pub compressed_count: u64,
    pub header_count: u64,
    pub log_count: u64,
}

pub(super) async fn perform_usage_cleanup_once(
    data: &GatewayDataState,
) -> Result<UsageCleanupSummary, DataLayerError> {
    perform_usage_cleanup_once_with_override(data, None).await
}

pub(super) async fn perform_usage_cleanup_once_with_override(
    data: &GatewayDataState,
    override_older_than: Option<chrono::Duration>,
) -> Result<UsageCleanupSummary, DataLayerError> {
    if !data.has_usage_writer() {
        return Ok(UsageCleanupSummary::default());
    }
    if override_older_than.is_none()
        && !system_config_bool(data, "enable_auto_cleanup", true).await?
    {
        return Ok(UsageCleanupSummary::default());
    }

    let window = compute_usage_cleanup_window(data, override_older_than).await?;
    let settings = usage_cleanup_settings(data).await?;
    data.cleanup_usage(
        &window,
        settings.batch_size,
        settings.auto_delete_expired_keys,
    )
    .await
}

pub(crate) async fn preview_manual_usage_cleanup(
    data: &GatewayDataState,
    override_older_than_days: Option<u32>,
) -> Result<ManualUsageCleanupPreview, DataLayerError> {
    let override_duration =
        override_older_than_days.map(|days| chrono::Duration::days(i64::from(days)));
    let window = compute_usage_cleanup_window(data, override_duration).await?;
    let counts = data.preview_usage_cleanup(&window).await?;
    Ok(ManualUsageCleanupPreview {
        detail_cutoff: window.detail_cutoff,
        compressed_cutoff: window.compressed_cutoff,
        header_cutoff: window.header_cutoff,
        log_cutoff: window.log_cutoff,
        requested_older_than_days: override_older_than_days,
        detail_count: counts.detail,
        compressed_count: counts.compressed,
        header_count: counts.header,
        log_count: counts.log,
    })
}

async fn compute_usage_cleanup_window(
    data: &GatewayDataState,
    override_older_than: Option<chrono::Duration>,
) -> Result<UsageCleanupWindow, DataLayerError> {
    let settings = usage_cleanup_settings(data).await?;
    Ok(match override_older_than {
        Some(duration) => usage_cleanup_window_with_override(Utc::now(), settings, Some(duration)),
        None => usage_cleanup_window(Utc::now(), settings),
    })
}
