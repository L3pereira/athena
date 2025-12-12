//! Snapshot Buffer
//!
//! Buffers snapshot requests from strategy to respect exchange rate limits.
//! Processes requests in FIFO order with configurable rate limiting.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use trading_core::SnapshotRequest;

/// Rate limiter for snapshot requests
struct RateLimiter {
    /// Minimum interval between requests
    min_interval: Duration,
    /// Last request time
    last_request: Option<Instant>,
}

impl RateLimiter {
    fn new(requests_per_second: u32) -> Self {
        let min_interval = if requests_per_second > 0 {
            Duration::from_secs(1) / requests_per_second
        } else {
            Duration::from_millis(100) // Default 10 req/s
        };

        RateLimiter {
            min_interval,
            last_request: None,
        }
    }

    fn can_proceed(&self) -> bool {
        match self.last_request {
            Some(last) => last.elapsed() >= self.min_interval,
            None => true,
        }
    }

    fn record_request(&mut self) {
        self.last_request = Some(Instant::now());
    }

    fn time_until_next(&self) -> Duration {
        match self.last_request {
            Some(last) => {
                let elapsed = last.elapsed();
                if elapsed >= self.min_interval {
                    Duration::ZERO
                } else {
                    self.min_interval - elapsed
                }
            }
            None => Duration::ZERO,
        }
    }
}

/// Buffers and rate-limits snapshot requests from strategy
pub struct SnapshotBuffer {
    /// Queue of pending snapshot requests (FIFO)
    pending_requests: VecDeque<SnapshotRequest>,
    /// Symbols currently in the queue (for deduplication)
    in_queue: HashSet<String>,
    /// Rate limiter
    rate_limiter: RateLimiter,
}

impl SnapshotBuffer {
    /// Create a new snapshot buffer with specified rate limit
    pub fn new(requests_per_second: u32) -> Self {
        SnapshotBuffer {
            pending_requests: VecDeque::new(),
            in_queue: HashSet::new(),
            rate_limiter: RateLimiter::new(requests_per_second),
        }
    }

    /// Request a snapshot for a symbol
    /// Returns true if the request was queued, false if already queued
    pub fn request_snapshot(&mut self, exchange: &str, symbol: &str) -> bool {
        let key = format!("{}:{}", exchange, symbol);
        if self.in_queue.contains(&key) {
            return false;
        }

        self.pending_requests
            .push_back(SnapshotRequest::new(exchange, symbol));
        self.in_queue.insert(key);
        true
    }

    /// Get the next request if rate limit allows
    pub fn get_next_if_ready(&mut self) -> Option<SnapshotRequest> {
        if !self.rate_limiter.can_proceed() {
            return None;
        }

        if let Some(request) = self.pending_requests.pop_front() {
            let key = format!("{}:{}", request.exchange, request.symbol);
            self.in_queue.remove(&key);
            self.rate_limiter.record_request();
            Some(request)
        } else {
            None
        }
    }

    /// Check if there are pending requests
    pub fn has_pending(&self) -> bool {
        !self.pending_requests.is_empty()
    }

    /// Get number of pending requests
    pub fn pending_count(&self) -> usize {
        self.pending_requests.len()
    }

    /// Get time until next request is allowed
    pub fn time_until_next(&self) -> Duration {
        self.rate_limiter.time_until_next()
    }

    /// Clear all pending requests
    pub fn clear(&mut self) {
        self.pending_requests.clear();
        self.in_queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_request_deduplication() {
        let mut buffer = SnapshotBuffer::new(10);

        assert!(buffer.request_snapshot("binance", "BTCUSDT"));
        assert!(!buffer.request_snapshot("binance", "BTCUSDT")); // Duplicate
        assert!(buffer.request_snapshot("binance", "ETHUSDT")); // Different symbol
        assert!(buffer.request_snapshot("kraken", "BTCUSDT")); // Different exchange

        assert_eq!(buffer.pending_count(), 3);
    }

    #[test]
    fn test_fifo_order() {
        let mut buffer = SnapshotBuffer::new(1000); // 1000 req/s = 1ms interval

        buffer.request_snapshot("binance", "BTCUSDT");
        buffer.request_snapshot("binance", "ETHUSDT");
        buffer.request_snapshot("binance", "XRPUSDT");

        let first = buffer.get_next_if_ready().unwrap();
        assert_eq!(first.symbol, "BTCUSDT");

        // Wait for rate limit to allow next request
        sleep(Duration::from_millis(2));
        let second = buffer.get_next_if_ready().unwrap();
        assert_eq!(second.symbol, "ETHUSDT");

        sleep(Duration::from_millis(2));
        let third = buffer.get_next_if_ready().unwrap();
        assert_eq!(third.symbol, "XRPUSDT");

        assert!(buffer.get_next_if_ready().is_none());
    }

    #[test]
    fn test_rate_limiting() {
        let mut buffer = SnapshotBuffer::new(100); // 100 req/s = 10ms interval

        buffer.request_snapshot("binance", "BTCUSDT");
        buffer.request_snapshot("binance", "ETHUSDT");

        // First request should proceed
        assert!(buffer.get_next_if_ready().is_some());

        // Second request should be rate limited
        assert!(buffer.get_next_if_ready().is_none());

        // Wait for rate limit to reset
        sleep(Duration::from_millis(15));

        // Now it should proceed
        assert!(buffer.get_next_if_ready().is_some());
    }

    #[test]
    fn test_can_requeue_after_processing() {
        let mut buffer = SnapshotBuffer::new(1000);

        buffer.request_snapshot("binance", "BTCUSDT");
        let _ = buffer.get_next_if_ready();

        // Should be able to queue same symbol again after processing
        assert!(buffer.request_snapshot("binance", "BTCUSDT"));
    }

    #[test]
    fn test_clear() {
        let mut buffer = SnapshotBuffer::new(10);

        buffer.request_snapshot("binance", "BTCUSDT");
        buffer.request_snapshot("binance", "ETHUSDT");
        assert_eq!(buffer.pending_count(), 2);

        buffer.clear();
        assert_eq!(buffer.pending_count(), 0);

        // Should be able to queue again after clear
        assert!(buffer.request_snapshot("binance", "BTCUSDT"));
    }
}
