//! Market Data Adapter - Adapts OrderBookManager to MarketDataPort
//!
//! This adapter implements the MarketDataPort and OrderBookReader traits
//! using the concrete OrderBookManager and SharedOrderBook types.

use crate::application::ports::{BookLevel, MarketDataPort, OrderBookReader, SymbolKey};
use crate::domain::order_book::{OrderBookManager, SharedOrderBook};
use std::sync::Arc;
use trading_core::QualifiedSymbol;
use trading_core::{Price, Quantity};

/// Adapter that implements OrderBookReader for SharedOrderBook
pub struct OrderBookReaderAdapter {
    book: SharedOrderBook,
}

impl OrderBookReaderAdapter {
    pub fn new(book: SharedOrderBook) -> Self {
        Self { book }
    }

    /// Get the underlying SharedOrderBook
    pub fn inner(&self) -> &SharedOrderBook {
        &self.book
    }
}

impl OrderBookReader for OrderBookReaderAdapter {
    fn is_initialized(&self) -> bool {
        self.book.is_initialized()
    }

    fn best_bid(&self) -> Option<BookLevel> {
        self.book.best_bid().map(|level| BookLevel {
            price: level.price,
            size: level.quantity,
        })
    }

    fn best_ask(&self) -> Option<BookLevel> {
        self.book.best_ask().map(|level| BookLevel {
            price: level.price,
            size: level.quantity,
        })
    }

    fn mid_price(&self) -> Option<Price> {
        self.book.mid_price()
    }

    fn spread(&self) -> Option<Price> {
        self.book.spread()
    }

    fn bid_levels(&self, depth: usize) -> Vec<BookLevel> {
        self.book
            .top_bids(depth)
            .into_iter()
            .map(|level| BookLevel {
                price: level.price,
                size: level.quantity,
            })
            .collect()
    }

    fn ask_levels(&self, depth: usize) -> Vec<BookLevel> {
        self.book
            .top_asks(depth)
            .into_iter()
            .map(|level| BookLevel {
                price: level.price,
                size: level.quantity,
            })
            .collect()
    }

    fn total_bid_depth(&self, levels: usize) -> Quantity {
        let sum: i64 = self
            .book
            .top_bids(levels)
            .into_iter()
            .map(|level| level.quantity.raw())
            .sum();
        Quantity::from_raw(sum)
    }

    fn total_ask_depth(&self, levels: usize) -> Quantity {
        let sum: i64 = self
            .book
            .top_asks(levels)
            .into_iter()
            .map(|level| level.quantity.raw())
            .sum();
        Quantity::from_raw(sum)
    }

    fn last_update_time(&self) -> Option<u64> {
        // The current SharedOrderBook doesn't track time, just update ID
        // We return the update ID as a proxy for time ordering
        Some(self.book.last_update_id())
    }
}

/// Adapter that implements MarketDataPort for OrderBookManager
pub struct MarketDataAdapter {
    manager: OrderBookManager,
}

impl MarketDataAdapter {
    pub fn new(manager: OrderBookManager) -> Self {
        Self { manager }
    }

    /// Get the underlying OrderBookManager
    pub fn inner(&self) -> &OrderBookManager {
        &self.manager
    }

    /// Convert SymbolKey to QualifiedSymbol
    fn to_qualified(key: &SymbolKey) -> QualifiedSymbol {
        QualifiedSymbol::new(key.exchange.clone(), key.symbol.clone())
    }

    /// Convert QualifiedSymbol to SymbolKey
    fn from_qualified(qs: &QualifiedSymbol) -> SymbolKey {
        SymbolKey::new(qs.exchange.as_str(), &qs.symbol)
    }
}

impl MarketDataPort for MarketDataAdapter {
    type BookReader = OrderBookReaderAdapter;

    fn book(&self, key: &SymbolKey) -> Arc<Self::BookReader> {
        let qs = Self::to_qualified(key);
        let book = self.manager.book_by_key(&qs);
        Arc::new(OrderBookReaderAdapter::new(book))
    }

    fn has_symbol(&self, key: &SymbolKey) -> bool {
        let qs = Self::to_qualified(key);
        self.manager.book_by_key(&qs).is_initialized()
    }

    fn symbols(&self) -> Vec<SymbolKey> {
        self.manager
            .symbols()
            .into_iter()
            .map(|qs| Self::from_qualified(&qs))
            .collect()
    }
}

/// Convenience function to create a market data adapter
pub fn adapt_market_data(manager: OrderBookManager) -> MarketDataAdapter {
    MarketDataAdapter::new(manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::{CompactLevel, OrderBookSnapshot};

    #[test]
    fn test_order_book_reader_adapter() {
        let manager = OrderBookManager::new();

        let snapshot = OrderBookSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 100,
            timestamp_ns: 0,
            bids: vec![
                CompactLevel::new(50000_00000000, 1_00000000),
                CompactLevel::new(49900_00000000, 2_00000000),
            ],
            asks: vec![
                CompactLevel::new(50100_00000000, 1_50000000),
                CompactLevel::new(50200_00000000, 2_50000000),
            ],
        };
        manager.apply_snapshot(&snapshot);

        let key = QualifiedSymbol::new("binance", "BTCUSDT");
        let book = manager.book_by_key(&key);
        let adapter = OrderBookReaderAdapter::new(book);

        assert!(adapter.is_initialized());

        let best_bid = adapter.best_bid().unwrap();
        assert_eq!(best_bid.price.raw(), 50000_00000000);
        assert_eq!(best_bid.size.raw(), 1_00000000);

        let best_ask = adapter.best_ask().unwrap();
        assert_eq!(best_ask.price.raw(), 50100_00000000);

        let mid = adapter.mid_price().unwrap();
        assert_eq!(mid.raw(), 50050_00000000);

        let spread = adapter.spread().unwrap();
        assert_eq!(spread.raw(), 100_00000000);

        let bids = adapter.bid_levels(2);
        assert_eq!(bids.len(), 2);
        assert_eq!(bids[0].price.raw(), 50000_00000000);
        assert_eq!(bids[1].price.raw(), 49900_00000000);

        let total_bid = adapter.total_bid_depth(2);
        assert_eq!(total_bid.raw(), 3_00000000); // 1 + 2
    }

    #[test]
    fn test_market_data_adapter() {
        let manager = OrderBookManager::new();

        let snapshot = OrderBookSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 100,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 1_00000000)],
            asks: vec![CompactLevel::new(50100_00000000, 1_50000000)],
        };
        manager.apply_snapshot(&snapshot);

        let adapter = MarketDataAdapter::new(manager);

        let symbol_key = SymbolKey::new("binance", "BTCUSDT");
        assert!(adapter.has_symbol(&symbol_key));

        let book = adapter.book(&symbol_key);
        assert!(book.is_initialized());
        assert_eq!(book.best_bid().unwrap().price.raw(), 50000_00000000);

        let symbols = adapter.symbols();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].symbol, "BTCUSDT");
    }
}
