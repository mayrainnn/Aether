use aether_runtime_state::{DataLayerError, RuntimeLockLease, RuntimeState};

pub(crate) async fn release_admin_provider_pool_key_lease(
    runtime: &RuntimeState,
    lease: &RuntimeLockLease,
) -> Result<bool, DataLayerError> {
    runtime.lock_release(lease).await
}
