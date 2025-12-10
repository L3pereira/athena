use std::time::Duration;

use crate::gateway_in::domain::ExchangeId;

/// Configuration for the market data handler
/// Application-level configuration
#[derive(Debug, Clone)]
pub struct MarketDataConfig {
    /// Exchange this handler is connected to
    pub exchange_id: ExchangeId,
    /// Minimum interval between snapshot requests (rate limiting)
    pub snapshot_interval: Duration,
    /// Maximum updates to buffer while waiting for snapshot
    pub max_buffer_size: usize,
    /// Symbols to subscribe to
    pub symbols: Vec<String>,
}

impl MarketDataConfig {
    pub fn new(exchange_id: impl Into<ExchangeId>) -> Self {
        MarketDataConfig {
            exchange_id: exchange_id.into(),
            snapshot_interval: Duration::from_millis(100),
            max_buffer_size: 1000,
            symbols: Vec::new(),
        }
    }

    pub fn with_symbols(mut self, symbols: Vec<String>) -> Self {
        self.symbols = symbols;
        self
    }

    pub fn with_snapshot_interval(mut self, interval: Duration) -> Self {
        self.snapshot_interval = interval;
        self
    }

    pub fn with_max_buffer_size(mut self, size: usize) -> Self {
        self.max_buffer_size = size;
        self
    }
}

/// Configuration for the gateway connection
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub rest_url: String,
    pub ws_url: String,
    pub api_key: String,
}

impl GatewayConfig {
    pub fn new(rest_url: String, ws_url: String, api_key: String) -> Self {
        GatewayConfig {
            rest_url,
            ws_url,
            api_key,
        }
    }
}
