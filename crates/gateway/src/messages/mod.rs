//! Wire message types for gateway communication
//!
//! These types are designed for efficient serialization with bincode
//! and represent the normalized format for market data and orders.

pub mod market_data;
pub mod order;

pub use market_data::{BookLevel, OrderBookUpdate, TradeMessage};
pub use order::{CancelRequest, OrderRequest, OrderResponse};
