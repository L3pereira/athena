//! Market data message types

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Order book level (price + quantity)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

impl BookLevel {
    /// Create a new book level
    pub fn new(price: Decimal, quantity: Decimal) -> Self {
        Self { price, quantity }
    }

    /// Check if this level should be removed (quantity == 0)
    pub fn is_removed(&self) -> bool {
        self.quantity.is_zero()
    }
}

/// Order book update message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderBookUpdate {
    /// Full snapshot of the order book
    Snapshot {
        instrument_id: String,
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
        sequence: u64,
        timestamp_ns: i64,
    },
    /// Incremental update (delta)
    Delta {
        instrument_id: String,
        /// Changed bid levels (qty=0 means remove level)
        bids: Vec<BookLevel>,
        /// Changed ask levels (qty=0 means remove level)
        asks: Vec<BookLevel>,
        sequence: u64,
        timestamp_ns: i64,
    },
}

impl OrderBookUpdate {
    /// Create a new snapshot update
    pub fn snapshot(
        instrument_id: impl Into<String>,
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
        sequence: u64,
        timestamp_ns: i64,
    ) -> Self {
        Self::Snapshot {
            instrument_id: instrument_id.into(),
            bids,
            asks,
            sequence,
            timestamp_ns,
        }
    }

    /// Create a new delta update
    pub fn delta(
        instrument_id: impl Into<String>,
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
        sequence: u64,
        timestamp_ns: i64,
    ) -> Self {
        Self::Delta {
            instrument_id: instrument_id.into(),
            bids,
            asks,
            sequence,
            timestamp_ns,
        }
    }

    /// Get the instrument ID
    pub fn instrument_id(&self) -> &str {
        match self {
            Self::Snapshot { instrument_id, .. } => instrument_id,
            Self::Delta { instrument_id, .. } => instrument_id,
        }
    }

    /// Get the sequence number
    pub fn sequence(&self) -> u64 {
        match self {
            Self::Snapshot { sequence, .. } => *sequence,
            Self::Delta { sequence, .. } => *sequence,
        }
    }

    /// Get the timestamp in nanoseconds
    pub fn timestamp_ns(&self) -> i64 {
        match self {
            Self::Snapshot { timestamp_ns, .. } => *timestamp_ns,
            Self::Delta { timestamp_ns, .. } => *timestamp_ns,
        }
    }

    /// Check if this is a snapshot
    pub fn is_snapshot(&self) -> bool {
        matches!(self, Self::Snapshot { .. })
    }
}

/// Aggressor side for trades
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggressorSide {
    Buy,
    Sell,
}

impl AggressorSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

/// Trade execution message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeMessage {
    pub instrument_id: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub aggressor_side: AggressorSide,
    pub timestamp_ns: i64,
    pub trade_id: String,
}

impl TradeMessage {
    /// Create a new trade message
    pub fn new(
        instrument_id: impl Into<String>,
        price: Decimal,
        quantity: Decimal,
        aggressor_side: AggressorSide,
        timestamp_ns: i64,
        trade_id: impl Into<String>,
    ) -> Self {
        Self {
            instrument_id: instrument_id.into(),
            price,
            quantity,
            aggressor_side,
            timestamp_ns,
            trade_id: trade_id.into(),
        }
    }

    /// Get the notional value of the trade
    pub fn notional(&self) -> Decimal {
        self.price * self.quantity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_book_level() {
        let level = BookLevel::new(dec!(100.0), dec!(10.0));
        assert!(!level.is_removed());

        let removed = BookLevel::new(dec!(100.0), dec!(0));
        assert!(removed.is_removed());
    }

    #[test]
    #[ignore = "bincode doesn't support rust_decimal's deserialize_any"]
    fn test_order_book_update_serialization() {
        let update = OrderBookUpdate::snapshot(
            "BTC-USD",
            vec![BookLevel::new(dec!(50000), dec!(1.5))],
            vec![BookLevel::new(dec!(50100), dec!(2.0))],
            1,
            1234567890,
        );

        let bytes = bincode::serialize(&update).unwrap();
        let decoded: OrderBookUpdate = bincode::deserialize(&bytes).unwrap();

        assert_eq!(decoded.instrument_id(), "BTC-USD");
        assert_eq!(decoded.sequence(), 1);
    }

    #[test]
    fn test_trade_message() {
        let trade = TradeMessage::new(
            "ETH-USD",
            dec!(3000),
            dec!(5.0),
            AggressorSide::Buy,
            1234567890,
            "trade-123",
        );

        assert_eq!(trade.notional(), dec!(15000));
    }
}
