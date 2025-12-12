//! Market Data Messages
//!
//! IPC message types for order book snapshots, depth updates, and trades.

use serde::{Deserialize, Serialize};

use crate::{Price, PriceLevel, Quantity};

/// Compact price level for efficient IPC serialization
///
/// Uses raw i64 values to avoid floating-point precision issues
/// and reduce serialization overhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactLevel {
    /// Price as raw i64 (8 decimal places)
    pub price_raw: i64,
    /// Quantity as raw i64 (8 decimal places)
    pub quantity_raw: i64,
}

impl CompactLevel {
    /// Create a new compact level
    pub fn new(price_raw: i64, quantity_raw: i64) -> Self {
        Self {
            price_raw,
            quantity_raw,
        }
    }

    /// Create from Price and Quantity
    pub fn from_types(price: Price, quantity: Quantity) -> Self {
        Self {
            price_raw: price.raw(),
            quantity_raw: quantity.raw(),
        }
    }

    /// Convert to PriceLevel
    pub fn to_price_level(self) -> PriceLevel {
        PriceLevel::new(
            Price::from_raw(self.price_raw),
            Quantity::from_raw(self.quantity_raw),
        )
    }
}

impl From<PriceLevel> for CompactLevel {
    fn from(level: PriceLevel) -> Self {
        Self::from_types(level.price, level.quantity)
    }
}

impl From<CompactLevel> for PriceLevel {
    fn from(level: CompactLevel) -> Self {
        level.to_price_level()
    }
}

/// Order book snapshot message
///
/// Full snapshot of order book state, typically sent:
/// - On initial connection
/// - When strategy requests a resync
/// - Periodically to allow late-joining subscribers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    /// Exchange identifier
    pub exchange: String,
    /// Trading symbol
    pub symbol: String,
    /// Last update ID (for sequence validation)
    pub last_update_id: u64,
    /// Timestamp in nanoseconds since epoch
    pub timestamp_ns: u64,
    /// Bid levels (price descending)
    pub bids: Vec<CompactLevel>,
    /// Ask levels (price ascending)
    pub asks: Vec<CompactLevel>,
}

impl OrderBookSnapshot {
    /// Create a new order book snapshot
    pub fn new(exchange: &str, symbol: &str, last_update_id: u64) -> Self {
        Self {
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            last_update_id,
            timestamp_ns: current_timestamp_ns(),
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }

    /// Add bid levels
    pub fn with_bids(mut self, bids: Vec<CompactLevel>) -> Self {
        self.bids = bids;
        self
    }

    /// Add ask levels
    pub fn with_asks(mut self, asks: Vec<CompactLevel>) -> Self {
        self.asks = asks;
        self
    }
}

/// Depth update message (delta)
///
/// Incremental update to order book state. Strategy must:
/// 1. Validate sequence (first_update_id <= expected <= final_update_id)
/// 2. Apply deltas to local order book
/// 3. Request snapshot on sequence gap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthUpdate {
    /// Exchange identifier
    pub exchange: String,
    /// Trading symbol
    pub symbol: String,
    /// First update ID in this batch
    pub first_update_id: u64,
    /// Final update ID in this batch
    pub final_update_id: u64,
    /// Timestamp in nanoseconds since epoch
    pub timestamp_ns: u64,
    /// Bid level updates (quantity=0 means remove)
    pub bids: Vec<CompactLevel>,
    /// Ask level updates (quantity=0 means remove)
    pub asks: Vec<CompactLevel>,
}

impl DepthUpdate {
    /// Create a new depth update
    pub fn new(exchange: &str, symbol: &str, first_update_id: u64, final_update_id: u64) -> Self {
        Self {
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            first_update_id,
            final_update_id,
            timestamp_ns: current_timestamp_ns(),
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }

    /// Add bid updates
    pub fn with_bids(mut self, bids: Vec<CompactLevel>) -> Self {
        self.bids = bids;
        self
    }

    /// Add ask updates
    pub fn with_asks(mut self, asks: Vec<CompactLevel>) -> Self {
        self.asks = asks;
        self
    }
}

/// Trade update message
///
/// Notification of an executed trade on the exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeUpdate {
    /// Exchange identifier
    pub exchange: String,
    /// Trading symbol
    pub symbol: String,
    /// Trade ID from exchange
    pub trade_id: u64,
    /// Trade price (raw i64)
    pub price_raw: i64,
    /// Trade quantity (raw i64)
    pub quantity_raw: i64,
    /// True if buyer was the maker (passive order)
    pub buyer_is_maker: bool,
    /// Timestamp in nanoseconds since epoch
    pub timestamp_ns: u64,
}

impl TradeUpdate {
    /// Create a new trade update
    pub fn new(
        exchange: &str,
        symbol: &str,
        trade_id: u64,
        price: Price,
        quantity: Quantity,
        buyer_is_maker: bool,
    ) -> Self {
        Self {
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            trade_id,
            price_raw: price.raw(),
            quantity_raw: quantity.raw(),
            buyer_is_maker,
            timestamp_ns: current_timestamp_ns(),
        }
    }

    /// Get price as Price type
    pub fn price(&self) -> Price {
        Price::from_raw(self.price_raw)
    }

    /// Get quantity as Quantity type
    pub fn quantity(&self) -> Quantity {
        Quantity::from_raw(self.quantity_raw)
    }
}

/// Snapshot request message
///
/// Sent from strategy to gateway to request a full snapshot
/// for a specific symbol (typically after sequence gap detection).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRequest {
    /// Exchange identifier
    pub exchange: String,
    /// Trading symbol
    pub symbol: String,
    /// Request timestamp in nanoseconds
    pub timestamp_ns: u64,
}

impl SnapshotRequest {
    /// Create a new snapshot request
    pub fn new(exchange: &str, symbol: &str) -> Self {
        Self {
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            timestamp_ns: current_timestamp_ns(),
        }
    }
}

/// Get current timestamp in nanoseconds
fn current_timestamp_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_level() {
        let level = CompactLevel::new(5000000000000, 100000000); // 50000.0 price, 1.0 qty
        let price_level = level.to_price_level();
        assert_eq!(price_level.price, Price::from_int(50000));
        assert_eq!(price_level.quantity, Quantity::from_int(1));
    }

    #[test]
    fn test_compact_level_roundtrip() {
        let original = PriceLevel::new(Price::from_f64(123.456), Quantity::from_f64(7.89));
        let compact = CompactLevel::from(original.clone());
        let back: PriceLevel = compact.into();
        assert_eq!(original, back);
    }

    #[test]
    fn test_order_book_snapshot() {
        let snapshot = OrderBookSnapshot::new("binance", "BTCUSDT", 12345)
            .with_bids(vec![CompactLevel::new(5000000000000, 100000000)])
            .with_asks(vec![CompactLevel::new(5001000000000, 200000000)]);

        assert_eq!(snapshot.exchange, "binance");
        assert_eq!(snapshot.symbol, "BTCUSDT");
        assert_eq!(snapshot.last_update_id, 12345);
        assert_eq!(snapshot.bids.len(), 1);
        assert_eq!(snapshot.asks.len(), 1);
    }

    #[test]
    fn test_depth_update() {
        let update = DepthUpdate::new("kraken", "ETHUSDT", 100, 105)
            .with_bids(vec![CompactLevel::new(300000000000, 500000000)]);

        assert_eq!(update.exchange, "kraken");
        assert_eq!(update.first_update_id, 100);
        assert_eq!(update.final_update_id, 105);
    }

    #[test]
    fn test_trade_update() {
        let trade = TradeUpdate::new(
            "binance",
            "BTCUSDT",
            999,
            Price::from_f64(50000.0),
            Quantity::from_f64(0.5),
            true,
        );

        assert_eq!(trade.exchange, "binance");
        assert_eq!(trade.trade_id, 999);
        assert_eq!(trade.price(), Price::from_f64(50000.0));
        assert_eq!(trade.quantity(), Quantity::from_f64(0.5));
        assert!(trade.buyer_is_maker);
    }

    #[test]
    fn test_snapshot_request() {
        let request = SnapshotRequest::new("binance", "BTCUSDT");
        assert_eq!(request.exchange, "binance");
        assert_eq!(request.symbol, "BTCUSDT");
    }
}
