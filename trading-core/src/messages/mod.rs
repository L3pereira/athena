//! IPC Message Types
//!
//! Defines message types for inter-process communication between
//! gateway, strategy, and other trading infrastructure components.
//!
//! # Message Categories
//!
//! - **Exchange Identifiers**: `ExchangeId`, `QualifiedSymbol`
//! - **Market Data**: `OrderBookSnapshot`, `DepthUpdate`, `TradeUpdate`
//! - **Signals**: `SignalMessage`
//! - **Requests**: `SnapshotRequest`

mod exchange;
mod market_data;
mod signal;

pub use exchange::{ExchangeId, QualifiedSymbol};
pub use market_data::{CompactLevel, DepthUpdate, OrderBookSnapshot, SnapshotRequest, TradeUpdate};
pub use signal::SignalMessage;
