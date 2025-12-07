use async_trait::async_trait;
use std::time::Duration;

/// Configuration for rate limiting (Binance-style)
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Requests per interval for REST API
    pub requests_per_minute: u32,
    /// Order rate limit per interval
    pub orders_per_second: u32,
    /// Order rate limit per day
    pub orders_per_day: u32,
    /// Raw requests weight limit per minute
    pub request_weight_per_minute: u32,
    /// WebSocket connections per IP
    pub ws_connections_per_ip: u32,
    /// WebSocket message rate per second
    pub ws_messages_per_second: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        // Binance-like defaults
        RateLimitConfig {
            requests_per_minute: 1200,
            orders_per_second: 10,
            orders_per_day: 200_000,
            request_weight_per_minute: 1200,
            ws_connections_per_ip: 5,
            ws_messages_per_second: 5,
        }
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Current usage
    pub current: u32,
    /// Maximum allowed
    pub limit: u32,
    /// Time until reset
    pub retry_after: Option<Duration>,
    /// Weight of this request
    pub weight: u32,
}

impl RateLimitResult {
    pub fn allowed(current: u32, limit: u32, weight: u32) -> Self {
        RateLimitResult {
            allowed: true,
            current,
            limit,
            retry_after: None,
            weight,
        }
    }

    pub fn denied(current: u32, limit: u32, retry_after: Duration, weight: u32) -> Self {
        RateLimitResult {
            allowed: false,
            current,
            limit,
            retry_after: Some(retry_after),
            weight,
        }
    }
}

// ============================================================================
// Focused Rate Limiter Traits (ISP-compliant)
// ============================================================================

/// Rate limiter for REST API requests (weight-based)
#[async_trait]
pub trait RequestRateLimiter: Send + Sync {
    /// Check if a request is allowed (consumes quota if allowed)
    async fn check_request(&self, client_id: &str, weight: u32) -> RateLimitResult;
}

/// Rate limiter for order submissions
#[async_trait]
pub trait OrderRateLimiter: Send + Sync {
    /// Check if an order submission is allowed
    async fn check_order(&self, client_id: &str) -> RateLimitResult;
}

/// Rate limiter for WebSocket messages
#[async_trait]
pub trait WebSocketRateLimiter: Send + Sync {
    /// Check WebSocket message rate
    async fn check_ws_message(&self, client_id: &str) -> RateLimitResult;
}

/// Rate limit administration (status, reset, config)
#[async_trait]
pub trait RateLimitAdmin: Send + Sync {
    /// Get current rate limit status for a client
    async fn get_status(&self, client_id: &str) -> RateLimitStatus;

    /// Reset rate limits (for testing)
    async fn reset(&self, client_id: &str);

    /// Get the configuration
    fn config(&self) -> &RateLimitConfig;
}

// ============================================================================
// Composite Trait (backwards compatible)
// ============================================================================

/// Full rate limiter combining all capabilities
///
/// Use focused traits when you only need a subset:
/// - `RequestRateLimiter` for REST API weight checks
/// - `OrderRateLimiter` for order submission checks
/// - `WebSocketRateLimiter` for WS message checks
/// - `RateLimitAdmin` for status/reset/config
#[async_trait]
pub trait RateLimiter:
    RequestRateLimiter + OrderRateLimiter + WebSocketRateLimiter + RateLimitAdmin
{
}

/// Current rate limit status for a client
#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    pub request_weight_used: u32,
    pub request_weight_limit: u32,
    pub orders_used_second: u32,
    pub orders_limit_second: u32,
    pub orders_used_day: u32,
    pub orders_limit_day: u32,
}

impl Default for RateLimitStatus {
    fn default() -> Self {
        RateLimitStatus {
            request_weight_used: 0,
            request_weight_limit: 1200,
            orders_used_second: 0,
            orders_limit_second: 10,
            orders_used_day: 0,
            orders_limit_day: 200_000,
        }
    }
}
