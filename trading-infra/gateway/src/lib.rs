//! Gateway Crate
//!
//! Market data gateway for connecting to exchanges and forwarding
//! order book updates to strategy processes.
//!
//! # Architecture
//!
//! The gateway:
//! - Connects to exchanges via REST and WebSocket
//! - Receives order book deltas from exchange streams
//! - Forwards deltas to strategy via transport layer
//! - Handles snapshot requests from strategy (with rate limiting)
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  Binance    │     │   Kraken    │     │    ...      │
//! │  Exchange   │     │  Exchange   │     │             │
//! └──────┬──────┘     └──────┬──────┘     └──────┬──────┘
//!        │ WebSocket         │                    │
//!        ▼                   ▼                    ▼
//! ┌──────────────────────────────────────────────────────┐
//! │                     Gateway                           │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │
//! │  │ ExchangeConn│  │ ExchangeConn│  │ ExchangeConn│   │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘   │
//! │         │                │                │          │
//! │         ▼                ▼                ▼          │
//! │  ┌────────────────────────────────────────────────┐  │
//! │  │            MarketDataPublisher                 │  │
//! │  │         (forwards deltas via transport)        │  │
//! │  └────────────────────────────────────────────────┘  │
//! │                         │                            │
//! │  ┌────────────────────────────────────────────────┐  │
//! │  │            SnapshotBuffer                      │  │
//! │  │    (rate-limited snapshot request handling)    │  │
//! │  └────────────────────────────────────────────────┘  │
//! └──────────────────────────┬───────────────────────────┘
//!                            │ Transport (Channel/Aeron)
//!                            ▼
//!                     ┌─────────────┐
//!                     │  Strategy   │
//!                     └─────────────┘
//! ```

pub mod application;
pub mod config;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

// Re-export key types
pub use domain::events::{StreamData, WsEvent, WsRequest, WsResponse};
pub use domain::sync_status::SyncStatus;
pub use domain::traits::{
    DepthFetcher, FetchError, OrderBookWriter, SnapshotWriter, StreamParser, UpdateWriter,
};
pub use domain::{ExchangeId, QualifiedSymbol};

pub use application::config::{GatewayConfig, MarketDataConfig};
pub use application::exchange_manager::ExchangeManager;
pub use application::market_data_handler::MarketDataHandler;
pub use application::snapshot_buffer::SnapshotBuffer;

pub use infrastructure::parsers::{DepthParser, StreamDataParser, TradeParser};
pub use infrastructure::rest_client::{RestClient, RestError};
pub use infrastructure::ws_client::{WsClient, WsRequestSender};

pub use presentation::MarketDataPublisher;

pub use config::{ExchangeConfig, GatewayConfigFile, load_config, load_default_config};
