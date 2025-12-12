//! Market Data Port - Abstraction for reading market data
//!
//! This port defines how the signal generation domain accesses market data.
//! Infrastructure layer provides concrete implementations.

use std::sync::Arc;
use trading_core::{Price, Quantity};

/// Abstraction for a single order book level
#[derive(Debug, Clone)]
pub struct BookLevel {
    pub price: Price,
    pub size: Quantity,
}

/// Abstraction for order book data needed by signal generation
///
/// This trait abstracts the order book reading so signal generators
/// don't depend on concrete order book implementations.
pub trait OrderBookReader: Send + Sync {
    /// Check if the book has been initialized with data
    fn is_initialized(&self) -> bool;

    /// Get the best bid price and size
    fn best_bid(&self) -> Option<BookLevel>;

    /// Get the best ask price and size
    fn best_ask(&self) -> Option<BookLevel>;

    /// Get the mid price
    fn mid_price(&self) -> Option<Price>;

    /// Get spread
    fn spread(&self) -> Option<Price>;

    /// Get N levels from bid side
    fn bid_levels(&self, depth: usize) -> Vec<BookLevel>;

    /// Get N levels from ask side
    fn ask_levels(&self, depth: usize) -> Vec<BookLevel>;

    /// Get total bid depth (sum of sizes)
    fn total_bid_depth(&self, levels: usize) -> Quantity;

    /// Get total ask depth (sum of sizes)
    fn total_ask_depth(&self, levels: usize) -> Quantity;

    /// Get the last update timestamp (microseconds)
    fn last_update_time(&self) -> Option<u64>;
}

/// Symbol identifier for market data lookups
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolKey {
    pub exchange: String,
    pub symbol: String,
}

impl SymbolKey {
    pub fn new(exchange: impl Into<String>, symbol: impl Into<String>) -> Self {
        Self {
            exchange: exchange.into(),
            symbol: symbol.into(),
        }
    }
}

impl std::fmt::Display for SymbolKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.exchange, self.symbol)
    }
}

/// Port for accessing market data across multiple symbols
///
/// This is the main entry point for signal generators to access order books.
/// Implementations should provide lock-free or low-contention access.
pub trait MarketDataPort: Send + Sync {
    /// Type of order book reader returned
    type BookReader: OrderBookReader;

    /// Get an order book reader for a symbol
    fn book(&self, key: &SymbolKey) -> Arc<Self::BookReader>;

    /// Check if a symbol exists in the market data
    fn has_symbol(&self, key: &SymbolKey) -> bool;

    /// Get all available symbols
    fn symbols(&self) -> Vec<SymbolKey>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_key() {
        let key = SymbolKey::new("binance", "BTCUSDT");
        assert_eq!(key.exchange, "binance");
        assert_eq!(key.symbol, "BTCUSDT");
        assert_eq!(key.to_string(), "binance:BTCUSDT");
    }
}
