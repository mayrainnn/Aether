use async_trait::async_trait;

pub const DEFAULT_POOL_WINDOW_SIZE: u32 = 16;
pub const DEFAULT_POOL_PAGE_SIZE: u32 = 64;
pub const DEFAULT_POOL_MAX_SCAN: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PoolWindowConfig {
    pub window_size: u32,
    pub page_size: u32,
    pub max_scan: u32,
}

impl Default for PoolWindowConfig {
    fn default() -> Self {
        Self {
            window_size: DEFAULT_POOL_WINDOW_SIZE,
            page_size: DEFAULT_POOL_PAGE_SIZE,
            max_scan: DEFAULT_POOL_MAX_SCAN,
        }
    }
}

impl PoolWindowConfig {
    pub fn normalized(self) -> Self {
        let page_size = self.page_size.max(1);
        let window_size = self.window_size.max(1).min(page_size);
        let max_scan = self.max_scan.max(window_size);
        Self {
            window_size,
            page_size,
            max_scan,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolDispatchWindow<Candidate> {
    pub candidates: Vec<Candidate>,
    pub scanned_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolDispatchCursorOutcome<Candidate> {
    pub candidates: Vec<Candidate>,
    pub scanned_count: u32,
    pub exhausted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PoolDispatchError<Error> {
    #[error("pool dispatch port failed")]
    Port(Error),
}

#[async_trait]
pub trait PoolDispatchPort {
    type Candidate: Send;
    type Error: Send;

    async fn read_page(
        &mut self,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Self::Candidate>, Self::Error>;

    async fn rank_and_filter_window(
        &mut self,
        candidates: Vec<Self::Candidate>,
        window_size: u32,
    ) -> Result<PoolDispatchWindow<Self::Candidate>, Self::Error>;
}

pub async fn run_pool_dispatch_cursor<Port>(
    port: &mut Port,
    config: PoolWindowConfig,
) -> Result<PoolDispatchCursorOutcome<Port::Candidate>, PoolDispatchError<Port::Error>>
where
    Port: PoolDispatchPort + Send,
{
    let config = config.normalized();
    let mut offset = 0_u32;
    let mut scanned_count = 0_u32;

    while scanned_count < config.max_scan {
        let limit = config.page_size.min(config.max_scan - scanned_count);
        let page = port
            .read_page(offset, limit)
            .await
            .map_err(PoolDispatchError::Port)?;
        if page.is_empty() {
            return Ok(PoolDispatchCursorOutcome {
                candidates: Vec::new(),
                scanned_count,
                exhausted: true,
            });
        }

        let page_len = u32::try_from(page.len()).unwrap_or(u32::MAX);
        offset = offset.saturating_add(page_len);
        scanned_count = scanned_count.saturating_add(page_len);

        let window = port
            .rank_and_filter_window(page, config.window_size)
            .await
            .map_err(PoolDispatchError::Port)?;
        if !window.candidates.is_empty() {
            return Ok(PoolDispatchCursorOutcome {
                candidates: window.candidates,
                scanned_count,
                exhausted: false,
            });
        }
    }

    Ok(PoolDispatchCursorOutcome {
        candidates: Vec::new(),
        scanned_count,
        exhausted: true,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use async_trait::async_trait;

    use super::{run_pool_dispatch_cursor, PoolDispatchPort, PoolDispatchWindow, PoolWindowConfig};

    #[derive(Default)]
    struct TestPort {
        pages: VecDeque<Vec<u32>>,
        read_limits: Vec<u32>,
    }

    #[async_trait]
    impl PoolDispatchPort for TestPort {
        type Candidate = u32;
        type Error = std::convert::Infallible;

        async fn read_page(
            &mut self,
            _offset: u32,
            limit: u32,
        ) -> Result<Vec<Self::Candidate>, Self::Error> {
            self.read_limits.push(limit);
            Ok(self.pages.pop_front().unwrap_or_default())
        }

        async fn rank_and_filter_window(
            &mut self,
            mut candidates: Vec<Self::Candidate>,
            window_size: u32,
        ) -> Result<PoolDispatchWindow<Self::Candidate>, Self::Error> {
            candidates.sort();
            candidates.truncate(window_size as usize);
            let scanned_count = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
            Ok(PoolDispatchWindow {
                candidates,
                scanned_count,
            })
        }
    }

    #[tokio::test]
    async fn small_pool_is_returned_in_one_frozen_window() {
        let mut port = TestPort {
            pages: VecDeque::from([vec![3, 1, 2]]),
            read_limits: Vec::new(),
        };

        let outcome = run_pool_dispatch_cursor(&mut port, PoolWindowConfig::default())
            .await
            .unwrap();

        assert_eq!(outcome.candidates, [1, 2, 3]);
        assert_eq!(outcome.scanned_count, 3);
        assert_eq!(port.read_limits, [64]);
    }

    #[tokio::test]
    async fn large_pool_returns_bounded_window() {
        let mut port = TestPort {
            pages: VecDeque::from([(0..100).rev().collect::<Vec<_>>()]),
            read_limits: Vec::new(),
        };

        let outcome = run_pool_dispatch_cursor(&mut port, PoolWindowConfig::default())
            .await
            .unwrap();

        assert_eq!(outcome.candidates.len(), 16);
        assert_eq!(outcome.candidates[0], 0);
        assert_eq!(outcome.candidates[15], 15);
        assert_eq!(port.read_limits, [64]);
    }

    #[tokio::test]
    async fn max_scan_caps_page_reads() {
        let mut port = TestPort {
            pages: VecDeque::from([Vec::new()]),
            read_limits: Vec::new(),
        };
        let config = PoolWindowConfig {
            window_size: 16,
            page_size: 64,
            max_scan: 32,
        };

        let outcome = run_pool_dispatch_cursor(&mut port, config).await.unwrap();

        assert!(outcome.exhausted);
        assert_eq!(port.read_limits, [32]);
    }
}
