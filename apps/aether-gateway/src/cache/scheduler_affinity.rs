use std::time::Duration;

use aether_cache::{ExpiringMap, ExpiringMapFreshEntry};
pub(crate) use aether_scheduler_core::SchedulerAffinityTarget;

#[derive(Debug, Default)]
pub(crate) struct SchedulerAffinityCache {
    entries: ExpiringMap<String, SchedulerAffinityCacheValue>,
}

#[derive(Debug, Clone)]
struct SchedulerAffinityCacheValue {
    target: SchedulerAffinityTarget,
    epoch: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct SchedulerAffinitySnapshotEntry {
    pub(crate) cache_key: String,
    pub(crate) target: SchedulerAffinityTarget,
    pub(crate) epoch: u64,
    pub(crate) age: Duration,
}

impl SchedulerAffinityCache {
    pub(crate) fn get_fresh(
        &self,
        cache_key: &str,
        ttl: Duration,
    ) -> Option<SchedulerAffinityTarget> {
        self.entries
            .get_fresh(&cache_key.to_string(), ttl)
            .map(|value| value.target)
    }

    pub(crate) fn get_fresh_for_epoch(
        &self,
        cache_key: &str,
        ttl: Duration,
        epoch: u64,
    ) -> Option<SchedulerAffinityTarget> {
        let value = self.entries.get_fresh(&cache_key.to_string(), ttl)?;
        (value.epoch == epoch).then_some(value.target)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn insert(
        &self,
        cache_key: String,
        target: SchedulerAffinityTarget,
        ttl: Duration,
        max_entries: usize,
    ) {
        self.insert_for_epoch(cache_key, target, ttl, max_entries, 0);
    }

    pub(crate) fn insert_for_epoch(
        &self,
        cache_key: String,
        target: SchedulerAffinityTarget,
        ttl: Duration,
        max_entries: usize,
        epoch: u64,
    ) {
        self.entries.insert(
            cache_key,
            SchedulerAffinityCacheValue { target, epoch },
            ttl,
            max_entries,
        );
    }

    pub(crate) fn remove(&self, cache_key: &str) -> Option<SchedulerAffinityTarget> {
        self.entries
            .remove(&cache_key.to_string())
            .map(|value| value.target)
    }

    pub(crate) fn clear(&self) {
        self.entries.clear();
    }

    pub(crate) fn fresh_entries(&self, ttl: Duration) -> Vec<SchedulerAffinitySnapshotEntry> {
        self.entries
            .snapshot_fresh(ttl)
            .into_iter()
            .map(
                |ExpiringMapFreshEntry { key, value, age }| SchedulerAffinitySnapshotEntry {
                    cache_key: key,
                    target: value.target,
                    epoch: value.epoch,
                    age,
                },
            )
            .collect()
    }

    pub(crate) fn fresh_entries_for_epoch(
        &self,
        ttl: Duration,
        epoch: u64,
    ) -> Vec<SchedulerAffinitySnapshotEntry> {
        self.fresh_entries(ttl)
            .into_iter()
            .filter(|entry| entry.epoch == epoch)
            .collect()
    }
}
