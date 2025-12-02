//! Athena Strategy Framework
//!
//! Provides the infrastructure for building trading strategies:
//! - Local order book replicas (no contention with gateway)
//! - Strategy trait for event-driven trading
//! - Market events for informed trading
//! - Built-in market making strategies
//!
//! ## Architecture
//!
//! ```text
//!                                  ┌────────────────┐
//!                                  │  Event Feed    │
//!                                  │ (Fair Value)   │
//!                                  └───────┬────────┘
//!                                          │ MarketEvent
//!                                          ▼
//! Gateway In ─────► delta stream ─────► LocalOrderBook (per strategy)
//!                                              │
//!                                              ▼
//!                                        ┌──────────┐
//!                                        │ Strategy │
//!                                        └────┬─────┘
//!                                              │ Actions
//!                                              ▼
//! Gateway Out ◄──── order requests ◄──── Order Sender
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use athena_strategy::{BasicMarketMaker, MarketMakerConfig, Strategy};
//!
//! let config = MarketMakerConfig {
//!     instrument_id: "BTC-USD".to_string(),
//!     spread_bps: dec!(10),
//!     ..Default::default()
//! };
//! let strategy = BasicMarketMaker::new(config);
//! ```

pub mod events;
pub mod market_maker;
pub mod mean_reversion;
pub mod orderbook;
pub mod strategy;

// Re-export main types
pub use events::MarketEvent;
pub use market_maker::{BasicMarketMaker, MarketMakerConfig};
pub use mean_reversion::{MeanReversionConfig, MeanReversionTaker};
pub use orderbook::LocalOrderBook;
pub use strategy::{Action, OpenOrder, Position, Strategy, StrategyContext};
