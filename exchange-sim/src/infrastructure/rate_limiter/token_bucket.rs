use crate::application::ports::{
    OrderRateLimiter, RateLimitAdmin, RateLimitConfig, RateLimitResult, RateLimitStatus,
    RateLimiter, RequestRateLimiter, WebSocketRateLimiter,
};
use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Token bucket rate limiter implementation
///
/// Simulates Binance-style rate limiting with:
/// - Request weight limits (per minute)
/// - Order limits (per second and per day)
/// - Per-client tracking
pub struct TokenBucketRateLimiter {
    config: RateLimitConfig,
    /// Per-client rate limit state
    clients: Arc<DashMap<String, ClientState>>,
}

struct ClientState {
    /// Request weight bucket
    request_weight: Mutex<TokenBucket>,
    /// Orders per second bucket
    orders_second: Mutex<TokenBucket>,
    /// Orders per day bucket
    orders_day: Mutex<TokenBucket>,
    /// WebSocket messages per second
    ws_messages: Mutex<TokenBucket>,
}

struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64, // tokens per second
    last_update: Instant,
}

impl TokenBucket {
    fn new(capacity: u32, refill_interval: Duration) -> Self {
        let refill_rate = capacity as f64 / refill_interval.as_secs_f64();
        TokenBucket {
            tokens: capacity as f64,
            capacity: capacity as f64,
            refill_rate,
            last_update: Instant::now(),
        }
    }

    fn try_consume(&mut self, amount: u32) -> (bool, Duration) {
        self.refill();

        let amount_f64 = amount as f64;
        if self.tokens >= amount_f64 {
            self.tokens -= amount_f64;
            (true, Duration::ZERO)
        } else {
            // Calculate wait time
            let deficit = amount_f64 - self.tokens;
            let wait_seconds = deficit / self.refill_rate;
            (false, Duration::from_secs_f64(wait_seconds))
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        let new_tokens = elapsed.as_secs_f64() * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_update = now;
    }

    fn current(&self) -> u32 {
        (self.capacity - self.tokens) as u32
    }

    fn limit(&self) -> u32 {
        self.capacity as u32
    }

    fn reset(&mut self) {
        self.tokens = self.capacity;
        self.last_update = Instant::now();
    }
}

impl ClientState {
    fn new(config: &RateLimitConfig) -> Self {
        ClientState {
            request_weight: Mutex::new(TokenBucket::new(
                config.request_weight_per_minute,
                Duration::from_secs(60),
            )),
            orders_second: Mutex::new(TokenBucket::new(
                config.orders_per_second,
                Duration::from_secs(1),
            )),
            orders_day: Mutex::new(TokenBucket::new(
                config.orders_per_day,
                Duration::from_secs(86400),
            )),
            ws_messages: Mutex::new(TokenBucket::new(
                config.ws_messages_per_second,
                Duration::from_secs(1),
            )),
        }
    }
}

impl TokenBucketRateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        TokenBucketRateLimiter {
            config,
            clients: Arc::new(DashMap::new()),
        }
    }

    fn get_or_create_client(
        &self,
        client_id: &str,
    ) -> dashmap::mapref::one::Ref<'_, String, ClientState> {
        if !self.clients.contains_key(client_id) {
            self.clients
                .insert(client_id.to_string(), ClientState::new(&self.config));
        }
        self.clients.get(client_id).unwrap()
    }
}

impl Default for TokenBucketRateLimiter {
    fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }
}

impl Clone for TokenBucketRateLimiter {
    fn clone(&self) -> Self {
        TokenBucketRateLimiter {
            config: self.config.clone(),
            clients: Arc::clone(&self.clients),
        }
    }
}

// Implement focused traits separately for better testability

#[async_trait]
impl RequestRateLimiter for TokenBucketRateLimiter {
    async fn check_request(&self, client_id: &str, weight: u32) -> RateLimitResult {
        let client = self.get_or_create_client(client_id);
        let mut bucket = client.request_weight.lock();

        let (allowed, retry_after) = bucket.try_consume(weight);

        if allowed {
            RateLimitResult::allowed(bucket.current(), bucket.limit(), weight)
        } else {
            RateLimitResult::denied(bucket.current(), bucket.limit(), retry_after, weight)
        }
    }
}

#[async_trait]
impl OrderRateLimiter for TokenBucketRateLimiter {
    async fn check_order(&self, client_id: &str) -> RateLimitResult {
        let client = self.get_or_create_client(client_id);

        // Check both per-second and per-day limits
        let mut second_bucket = client.orders_second.lock();
        let (second_ok, second_retry) = second_bucket.try_consume(1);

        if !second_ok {
            return RateLimitResult::denied(
                second_bucket.current(),
                second_bucket.limit(),
                second_retry,
                1,
            );
        }

        let mut day_bucket = client.orders_day.lock();
        let (day_ok, day_retry) = day_bucket.try_consume(1);

        if !day_ok {
            // Refund the second bucket since we're denying
            second_bucket.tokens += 1.0;
            return RateLimitResult::denied(day_bucket.current(), day_bucket.limit(), day_retry, 1);
        }

        RateLimitResult::allowed(second_bucket.current(), second_bucket.limit(), 1)
    }
}

#[async_trait]
impl WebSocketRateLimiter for TokenBucketRateLimiter {
    async fn check_ws_message(&self, client_id: &str) -> RateLimitResult {
        let client = self.get_or_create_client(client_id);
        let mut bucket = client.ws_messages.lock();

        let (allowed, retry_after) = bucket.try_consume(1);

        if allowed {
            RateLimitResult::allowed(bucket.current(), bucket.limit(), 1)
        } else {
            RateLimitResult::denied(bucket.current(), bucket.limit(), retry_after, 1)
        }
    }
}

#[async_trait]
impl RateLimitAdmin for TokenBucketRateLimiter {
    async fn get_status(&self, client_id: &str) -> RateLimitStatus {
        let client = self.get_or_create_client(client_id);

        let request_weight = client.request_weight.lock();
        let orders_second = client.orders_second.lock();
        let orders_day = client.orders_day.lock();

        RateLimitStatus {
            request_weight_used: request_weight.current(),
            request_weight_limit: request_weight.limit(),
            orders_used_second: orders_second.current(),
            orders_limit_second: orders_second.limit(),
            orders_used_day: orders_day.current(),
            orders_limit_day: orders_day.limit(),
        }
    }

    async fn reset(&self, client_id: &str) {
        if let Some(client) = self.clients.get(client_id) {
            client.request_weight.lock().reset();
            client.orders_second.lock().reset();
            client.orders_day.lock().reset();
            client.ws_messages.lock().reset();
        }
    }

    fn config(&self) -> &RateLimitConfig {
        &self.config
    }
}

// Composite trait implementation (backwards compatible)
#[async_trait]
impl RateLimiter for TokenBucketRateLimiter {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_request_limit() {
        let limiter = TokenBucketRateLimiter::new(RateLimitConfig {
            request_weight_per_minute: 10,
            ..Default::default()
        });

        // Should allow first requests
        for _ in 0..10 {
            let result = limiter.check_request("test", 1).await;
            assert!(result.allowed);
        }

        // Should deny when limit exceeded
        let result = limiter.check_request("test", 1).await;
        assert!(!result.allowed);
        assert!(result.retry_after.is_some());
    }

    #[tokio::test]
    async fn test_order_limit() {
        let limiter = TokenBucketRateLimiter::new(RateLimitConfig {
            orders_per_second: 2,
            orders_per_day: 100,
            ..Default::default()
        });

        // Should allow first orders
        assert!(limiter.check_order("test").await.allowed);
        assert!(limiter.check_order("test").await.allowed);

        // Should deny third order (per second limit)
        let result = limiter.check_order("test").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_per_client_isolation() {
        let limiter = TokenBucketRateLimiter::new(RateLimitConfig {
            request_weight_per_minute: 5,
            ..Default::default()
        });

        // Exhaust client1's limit
        for _ in 0..5 {
            limiter.check_request("client1", 1).await;
        }
        assert!(!limiter.check_request("client1", 1).await.allowed);

        // client2 should still have quota
        assert!(limiter.check_request("client2", 1).await.allowed);
    }
}
