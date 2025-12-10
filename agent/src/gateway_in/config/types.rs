use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Root configuration for the gateway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfigFile {
    pub exchanges: Vec<ExchangeConfig>,
    #[serde(default)]
    pub global: GlobalConfig,
}

/// Configuration for a single exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    /// Unique identifier for the exchange (e.g., "binance", "kraken")
    pub id: String,
    /// Display name
    pub name: String,
    /// Whether this exchange is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// REST API base URL
    pub rest_url: String,
    /// WebSocket URL
    pub ws_url: String,
    /// API key for authentication
    #[serde(default)]
    pub api_key: String,
    /// API secret for signing requests
    #[serde(default)]
    pub api_secret: String,
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limits: RateLimitConfig,
    /// Symbols to subscribe to
    #[serde(default)]
    pub symbols: Vec<String>,
    /// Market data handling configuration
    #[serde(default)]
    pub market_data: MarketDataConfigJson,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_requests_per_second")]
    pub requests_per_second: u32,
    #[serde(default = "default_orders_per_second")]
    pub orders_per_second: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            requests_per_second: default_requests_per_second(),
            orders_per_second: default_orders_per_second(),
        }
    }
}

/// Market data configuration (JSON representation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataConfigJson {
    #[serde(default = "default_snapshot_interval")]
    pub snapshot_interval_ms: u64,
    #[serde(default = "default_max_buffer_size")]
    pub max_buffer_size: usize,
}

impl Default for MarketDataConfigJson {
    fn default() -> Self {
        MarketDataConfigJson {
            snapshot_interval_ms: default_snapshot_interval(),
            max_buffer_size: default_max_buffer_size(),
        }
    }
}

impl MarketDataConfigJson {
    /// Convert to application-layer MarketDataConfig
    pub fn to_market_data_config(
        &self,
        exchange_id: impl Into<crate::gateway_in::ExchangeId>,
        symbols: Vec<String>,
    ) -> crate::gateway_in::MarketDataConfig {
        crate::gateway_in::MarketDataConfig {
            exchange_id: exchange_id.into(),
            snapshot_interval: Duration::from_millis(self.snapshot_interval_ms),
            max_buffer_size: self.max_buffer_size,
            symbols,
        }
    }
}

/// Global configuration that applies to all exchanges
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Delay between reconnection attempts in milliseconds
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_ms: u64,
    /// Maximum number of reconnection attempts
    #[serde(default = "default_max_reconnect_attempts")]
    pub max_reconnect_attempts: u32,
    /// Heartbeat interval in milliseconds
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_ms: u64,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        GlobalConfig {
            reconnect_delay_ms: default_reconnect_delay(),
            max_reconnect_attempts: default_max_reconnect_attempts(),
            heartbeat_interval_ms: default_heartbeat_interval(),
        }
    }
}

impl GlobalConfig {
    pub fn reconnect_delay(&self) -> Duration {
        Duration::from_millis(self.reconnect_delay_ms)
    }

    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.heartbeat_interval_ms)
    }
}

// Default value functions for serde
fn default_true() -> bool {
    true
}

fn default_requests_per_second() -> u32 {
    10
}

fn default_orders_per_second() -> u32 {
    5
}

fn default_snapshot_interval() -> u64 {
    100
}

fn default_max_buffer_size() -> usize {
    1000
}

fn default_reconnect_delay() -> u64 {
    5000
}

fn default_max_reconnect_attempts() -> u32 {
    10
}

fn default_heartbeat_interval() -> u64 {
    30000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_exchange_config() {
        let json = r#"{
            "id": "binance",
            "name": "Binance",
            "enabled": true,
            "rest_url": "https://api.binance.com",
            "ws_url": "wss://stream.binance.com:9443/ws",
            "symbols": ["BTCUSDT", "ETHUSDT"]
        }"#;

        let config: ExchangeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.id, "binance");
        assert_eq!(config.symbols.len(), 2);
        assert!(config.enabled);
    }

    #[test]
    fn test_deserialize_full_config() {
        let json = r#"{
            "exchanges": [
                {
                    "id": "simulator",
                    "name": "Simulator",
                    "rest_url": "http://localhost:8080",
                    "ws_url": "ws://localhost:8080/ws",
                    "symbols": ["BTCUSDT"]
                }
            ],
            "global": {
                "reconnect_delay_ms": 3000
            }
        }"#;

        let config: GatewayConfigFile = serde_json::from_str(json).unwrap();
        assert_eq!(config.exchanges.len(), 1);
        assert_eq!(config.global.reconnect_delay_ms, 3000);
    }

    #[test]
    fn test_defaults() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "rest_url": "http://localhost",
            "ws_url": "ws://localhost"
        }"#;

        let config: ExchangeConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.rate_limits.requests_per_second, 10);
        assert_eq!(config.market_data.snapshot_interval_ms, 100);
    }
}
