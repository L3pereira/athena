//! Gateway module for connecting to multiple exchanges.
//!
//! Follows Clean Architecture with four layers:
//! - **Config**: JSON-based configuration for multiple exchanges
//! - **Domain**: Core business logic, traits, and events
//! - **Application**: Use cases and orchestration (MarketDataHandler, ExchangeManager)
//! - **Infrastructure**: External dependencies (REST, WebSocket clients)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         Config Layer                            │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │  gateway_config.json                                     │   │
//! │  │  - Multiple exchange configurations                      │   │
//! │  │  - Rate limits, symbols, credentials                     │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! │                            │                                    │
//! │                            ▼                                    │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │                    Application                           │   │
//! │  │  - ExchangeManager (multi-exchange orchestration)        │   │
//! │  │  - MarketDataHandler (per-exchange sync)                 │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! │                            │                                    │
//! │                            ▼                                    │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │                      Domain                              │   │
//! │  │  - ExchangeId, QualifiedSymbol                           │   │
//! │  │  - DepthFetcher, OrderBookWriter, StreamParser traits    │   │
//! │  │  - StreamData, WsEvent, SyncStatus                       │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! │                            │                                    │
//! │                            ▼                                    │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │                   Infrastructure                         │   │
//! │  │  - RestClient (implements DepthFetcher)                  │   │
//! │  │  - WsClient                                              │   │
//! │  │  - DepthParser, TradeParser                              │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example Configuration
//!
//! ```json
//! {
//!   "exchanges": [
//!     {
//!       "id": "binance",
//!       "name": "Binance",
//!       "enabled": true,
//!       "rest_url": "https://api.binance.com",
//!       "ws_url": "wss://stream.binance.com:9443/ws",
//!       "symbols": ["BTCUSDT", "ETHUSDT"]
//!     }
//!   ]
//! }
//! ```

pub mod application;
pub mod config;
pub mod domain;
pub mod infrastructure;

// Re-export commonly used types for convenience

// Config layer
pub use config::{
    ConfigError, ExchangeConfig, GatewayConfigFile, GlobalConfig, MarketDataConfigJson,
    RateLimitConfig, load_config, load_config_from_str, load_default_config,
};

// Domain layer
pub use domain::{
    DepthFetcher, ExchangeId, OrderBookWriter, QualifiedSymbol, StreamData, StreamParser,
    SyncStatus, WsEvent, WsRequest, WsResponse,
};

// Application layer
pub use application::{ExchangeManager, GatewayConfig, MarketDataConfig, MarketDataHandler};

// Infrastructure layer
pub use infrastructure::{
    DepthParser, NewOrderRequest, OrderResponse, RestClient, RestError, TradeParser, WsClient,
    WsError, WsRequestSender,
};

use trading_core::{DepthSnapshotEvent, DepthUpdateEvent, TradeExecutedEvent};

/// High-level market events (for external consumers)
#[derive(Debug, Clone)]
pub enum MarketEvent {
    DepthUpdate(DepthUpdateEvent),
    DepthSnapshot(DepthSnapshotEvent),
    Trade(TradeExecutedEvent),
    Connected,
    Disconnected,
    Error(String),
}

/// Gateway facade - convenience wrapper for a single exchange connection
/// For multi-exchange support, use ExchangeManager instead
pub struct Gateway {
    config: GatewayConfig,
    rest_client: RestClient,
}

impl Gateway {
    pub fn new(config: GatewayConfig) -> Self {
        let rest_client = RestClient::new(config.rest_url.clone(), config.api_key.clone());

        Gateway {
            config,
            rest_client,
        }
    }

    pub fn config(&self) -> &GatewayConfig {
        &self.config
    }

    pub fn rest(&self) -> &RestClient {
        &self.rest_client
    }

    /// Create a WebSocket client for streaming market data
    pub fn create_ws_client(&self) -> WsClient {
        WsClient::new(self.config.ws_url.clone())
    }
}
