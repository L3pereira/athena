//! Strategy Crate
//!
//! Signal generation and feature extraction for trading strategies.
//!
//! # Architecture
//!
//! The strategy process:
//! - Receives order book deltas from gateway via transport
//! - Builds and maintains local order book state
//! - Extracts features from order book data
//! - Generates trading signals
//! - Publishes signals to execution layer
//!
//! ```text
//!                     ┌─────────────┐
//!                     │   Gateway   │
//!                     └──────┬──────┘
//!                            │ Transport (Channel/Aeron)
//!                            ▼
//! ┌──────────────────────────────────────────────────────┐
//! │                     Strategy                          │
//! │  ┌────────────────────────────────────────────────┐  │
//! │  │           MarketDataSubscriber                 │  │
//! │  │         (receives deltas, requests snapshots)  │  │
//! │  └────────────────────────────────────────────────┘  │
//! │                         │                            │
//! │                         ▼                            │
//! │  ┌────────────────────────────────────────────────┐  │
//! │  │              OrderBookManager                  │  │
//! │  │        (local order book, built from deltas)   │  │
//! │  └────────────────────────────────────────────────┘  │
//! │                         │                            │
//! │                         ▼                            │
//! │  ┌────────────────────────────────────────────────┐  │
//! │  │            Feature Extractors                  │  │
//! │  │   (OrderBookExtractor, PriceExtractor, etc.)   │  │
//! │  └────────────────────────────────────────────────┘  │
//! │                         │                            │
//! │                         ▼                            │
//! │  ┌────────────────────────────────────────────────┐  │
//! │  │           Signal Generators                    │  │
//! │  │    (MeanReversion, Momentum, Arbitrage, etc.)  │  │
//! │  └────────────────────────────────────────────────┘  │
//! │                         │                            │
//! │  ┌────────────────────────────────────────────────┐  │
//! │  │            SignalPublisher                     │  │
//! │  │         (publishes signals via transport)      │  │
//! │  └────────────────────────────────────────────────┘  │
//! └──────────────────────────┬───────────────────────────┘
//!                            │ Transport
//!                            ▼
//!                     ┌─────────────┐
//!                     │  Execution  │
//!                     └─────────────┘
//! ```

// Clean Architecture layers
pub mod application;
pub mod domain;
pub mod infrastructure;

// Re-export key types from domain
pub use domain::order_book::{OrderBookManager, SharedOrderBook};

// Re-export key types from infrastructure
pub use infrastructure::subscriber::MarketDataSubscriber;
