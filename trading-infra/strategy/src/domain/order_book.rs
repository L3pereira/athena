//! Order Book Manager for Strategy
//!
//! Builds and maintains local order book state from deltas received via transport.
//! Uses lock-free reads via ArcSwap for high-performance concurrent access.

use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::collections::BTreeMap;
use std::sync::Arc;
#[allow(unused_imports)] // Used in tests
use trading_core::CompactLevel;
use trading_core::{
    DepthUpdate, ExchangeId, OrderBookSnapshot, Price, PriceLevel, QualifiedSymbol, Quantity,
};

/// Order book state - immutable once created (copy-on-write)
#[derive(Clone, Debug, Default)]
struct OrderBookState {
    bids: BTreeMap<i64, i64>, // price_raw -> quantity_raw (descending for best bid)
    asks: BTreeMap<i64, i64>, // price_raw -> quantity_raw (ascending for best ask)
    last_update_id: u64,
    initialized: bool,
}

/// Multi-exchange order book manager with lock-free reads.
///
/// Architecture:
/// - `DashMap`: Per-symbol sharding, different symbols don't block each other
/// - `ArcSwap`: Lock-free reads, writers do copy-on-write
///
/// ```text
/// Reader 1 ──► load() ──► Arc<State> ──► read (never blocked)
/// Reader 2 ──► load() ──► Arc<State> ──► read (never blocked)
/// Writer   ──► clone + modify + store() ──► atomic swap
/// ```
#[derive(Clone)]
pub struct OrderBookManager {
    books: Arc<DashMap<QualifiedSymbol, Arc<ArcSwap<OrderBookState>>>>,
}

impl OrderBookManager {
    pub fn new() -> Self {
        OrderBookManager {
            books: Arc::new(DashMap::new()),
        }
    }

    /// Get or create an ArcSwap entry for a symbol
    fn get_or_create_swap(&self, key: &QualifiedSymbol) -> Arc<ArcSwap<OrderBookState>> {
        // Fast path: check if exists
        if let Some(entry) = self.books.get(key) {
            return Arc::clone(&entry);
        }

        // Slow path: create new entry
        self.books
            .entry(key.clone())
            .or_insert_with(|| Arc::new(ArcSwap::from_pointee(OrderBookState::default())))
            .clone()
    }

    /// Get a handle to a specific order book by exchange and symbol
    pub fn book(
        &self,
        exchange: impl Into<ExchangeId>,
        symbol: impl Into<String>,
    ) -> SharedOrderBook {
        let key = QualifiedSymbol::new(exchange, symbol);
        SharedOrderBook {
            swap: self.get_or_create_swap(&key),
            key,
        }
    }

    /// Get a handle to a specific order book by qualified symbol
    pub fn book_by_key(&self, key: &QualifiedSymbol) -> SharedOrderBook {
        SharedOrderBook {
            swap: self.get_or_create_swap(key),
            key: key.clone(),
        }
    }

    /// Apply a snapshot from gateway (IPC format)
    pub fn apply_snapshot(&self, snapshot: &OrderBookSnapshot) {
        let key = QualifiedSymbol::new(snapshot.exchange.clone(), snapshot.symbol.clone());
        let swap = self.get_or_create_swap(&key);

        // Build new state
        let mut new_state = OrderBookState {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_update_id: snapshot.last_update_id,
            initialized: true,
        };

        for level in &snapshot.bids {
            if level.quantity_raw != 0 {
                new_state.bids.insert(level.price_raw, level.quantity_raw);
            }
        }

        for level in &snapshot.asks {
            if level.quantity_raw != 0 {
                new_state.asks.insert(level.price_raw, level.quantity_raw);
            }
        }

        // Atomic swap - readers see old or new, never partial
        swap.store(Arc::new(new_state));
    }

    /// Apply a depth update delta from gateway (IPC format)
    /// Returns true if update was applied successfully, false if out of sync
    pub fn apply_delta(&self, update: &DepthUpdate) -> bool {
        let key = QualifiedSymbol::new(update.exchange.clone(), update.symbol.clone());

        let Some(swap) = self.books.get(&key) else {
            return false;
        };

        // Load current state (lock-free)
        let current = swap.load();

        if !current.initialized {
            return false;
        }

        // Sequence check
        let expected = current.last_update_id + 1;
        if update.first_update_id > expected || update.final_update_id < expected {
            return false;
        }

        // Clone and modify (copy-on-write)
        let mut new_state = (**current).clone();

        for level in &update.bids {
            if level.quantity_raw == 0 {
                new_state.bids.remove(&level.price_raw);
            } else {
                new_state.bids.insert(level.price_raw, level.quantity_raw);
            }
        }

        for level in &update.asks {
            if level.quantity_raw == 0 {
                new_state.asks.remove(&level.price_raw);
            } else {
                new_state.asks.insert(level.price_raw, level.quantity_raw);
            }
        }

        new_state.last_update_id = update.final_update_id;

        // Atomic swap
        swap.store(Arc::new(new_state));
        true
    }

    /// List all qualified symbols with initialized books
    pub fn symbols(&self) -> Vec<QualifiedSymbol> {
        self.books
            .iter()
            .filter(|entry| entry.value().load().initialized)
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// List all symbols for a specific exchange
    pub fn symbols_for_exchange(&self, exchange_id: &ExchangeId) -> Vec<String> {
        self.books
            .iter()
            .filter(|entry| {
                entry.key().exchange == *exchange_id && entry.value().load().initialized
            })
            .map(|entry| entry.key().symbol.clone())
            .collect()
    }
}

impl Default for OrderBookManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to a single order book - all reads are lock-free
#[derive(Clone)]
pub struct SharedOrderBook {
    swap: Arc<ArcSwap<OrderBookState>>,
    key: QualifiedSymbol,
}

impl SharedOrderBook {
    /// Get the qualified symbol this book is for
    pub fn qualified_symbol(&self) -> &QualifiedSymbol {
        &self.key
    }

    /// Get the exchange this book belongs to
    pub fn exchange(&self) -> &ExchangeId {
        &self.key.exchange
    }

    /// Get the symbol this book is for
    pub fn symbol(&self) -> &str {
        &self.key.symbol
    }

    /// Load current state snapshot (lock-free)
    #[inline]
    fn load(&self) -> arc_swap::Guard<Arc<OrderBookState>> {
        self.swap.load()
    }

    /// Get the best bid (highest buy price) - lock-free
    pub fn best_bid(&self) -> Option<PriceLevel> {
        let state = self.load();
        state
            .bids
            .iter()
            .next_back()
            .map(|(&p, &q)| PriceLevel::new(Price::from_raw(p), Quantity::from_raw(q)))
    }

    /// Get the best ask (lowest sell price) - lock-free
    pub fn best_ask(&self) -> Option<PriceLevel> {
        let state = self.load();
        state
            .asks
            .iter()
            .next()
            .map(|(&p, &q)| PriceLevel::new(Price::from_raw(p), Quantity::from_raw(q)))
    }

    /// Get the mid price - lock-free
    pub fn mid_price(&self) -> Option<Price> {
        let state = self.load();
        let best_bid = *state.bids.iter().next_back()?.0;
        let best_ask = *state.asks.iter().next()?.0;
        let mid = (best_bid + best_ask) / 2;
        Some(Price::from_raw(mid))
    }

    /// Get the spread (best ask - best bid) - lock-free
    pub fn spread(&self) -> Option<Price> {
        let state = self.load();
        let best_bid = *state.bids.iter().next_back()?.0;
        let best_ask = *state.asks.iter().next()?.0;
        Some(Price::from_raw(best_ask - best_bid))
    }

    /// Get top N bid levels - lock-free
    pub fn top_bids(&self, n: usize) -> Vec<PriceLevel> {
        let state = self.load();
        state
            .bids
            .iter()
            .rev()
            .take(n)
            .map(|(&p, &q)| PriceLevel::new(Price::from_raw(p), Quantity::from_raw(q)))
            .collect()
    }

    /// Get top N ask levels - lock-free
    pub fn top_asks(&self, n: usize) -> Vec<PriceLevel> {
        let state = self.load();
        state
            .asks
            .iter()
            .take(n)
            .map(|(&p, &q)| PriceLevel::new(Price::from_raw(p), Quantity::from_raw(q)))
            .collect()
    }

    /// Get the last update ID - lock-free
    pub fn last_update_id(&self) -> u64 {
        self.load().last_update_id
    }

    /// Check if the book is initialized - lock-free
    pub fn is_initialized(&self) -> bool {
        self.load().initialized
    }

    /// Get total bid volume up to a price level - lock-free
    pub fn bid_volume_to_price(&self, price: Price) -> Quantity {
        let state = self.load();
        let price_raw = price.raw();
        let total: i64 = state.bids.range(price_raw..).map(|(_, &q)| q).sum();
        Quantity::from_raw(total)
    }

    /// Get total ask volume up to a price level - lock-free
    pub fn ask_volume_to_price(&self, price: Price) -> Quantity {
        let state = self.load();
        let price_raw = price.raw();
        let total: i64 = state.asks.range(..=price_raw).map(|(_, &q)| q).sum();
        Quantity::from_raw(total)
    }

    /// Get a consistent snapshot of bids and asks - lock-free
    /// Useful when you need both sides atomically
    pub fn snapshot(&self) -> (Vec<PriceLevel>, Vec<PriceLevel>) {
        let state = self.load();
        let bids = state
            .bids
            .iter()
            .rev()
            .map(|(&p, &q)| PriceLevel::new(Price::from_raw(p), Quantity::from_raw(q)))
            .collect();
        let asks = state
            .asks
            .iter()
            .map(|(&p, &q)| PriceLevel::new(Price::from_raw(p), Quantity::from_raw(q)))
            .collect();
        (bids, asks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_snapshot() {
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

        let book = manager.book("binance", "BTCUSDT");
        assert!(book.is_initialized());
        assert_eq!(book.last_update_id(), 100);

        let bid = book.best_bid().unwrap();
        assert_eq!(bid.price.raw(), 50000_00000000);
        assert_eq!(bid.quantity.raw(), 1_00000000);
    }

    #[test]
    fn test_apply_delta() {
        let manager = OrderBookManager::new();

        // First apply snapshot
        let snapshot = OrderBookSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 100,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 1_00000000)],
            asks: vec![CompactLevel::new(50100_00000000, 1_50000000)],
        };
        manager.apply_snapshot(&snapshot);

        // Then apply delta
        let update = DepthUpdate {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            first_update_id: 101,
            final_update_id: 101,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 2_00000000)],
            asks: vec![],
        };

        assert!(manager.apply_delta(&update));

        let book = manager.book("binance", "BTCUSDT");
        assert_eq!(book.last_update_id(), 101);
        assert_eq!(book.best_bid().unwrap().quantity.raw(), 2_00000000);
    }

    #[test]
    fn test_delta_removes_level() {
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
            asks: vec![],
        };
        manager.apply_snapshot(&snapshot);

        // Remove best bid by setting quantity to 0
        let update = DepthUpdate {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            first_update_id: 101,
            final_update_id: 101,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 0)],
            asks: vec![],
        };

        assert!(manager.apply_delta(&update));

        let book = manager.book("binance", "BTCUSDT");
        // Best bid should now be 49900
        assert_eq!(book.best_bid().unwrap().price.raw(), 49900_00000000);
    }

    #[test]
    fn test_out_of_sequence_rejected() {
        let manager = OrderBookManager::new();

        let snapshot = OrderBookSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 100,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 1_00000000)],
            asks: vec![],
        };
        manager.apply_snapshot(&snapshot);

        // Out of sequence (expected 101, got 105)
        let update = DepthUpdate {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            first_update_id: 105,
            final_update_id: 106,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 2_00000000)],
            asks: vec![],
        };

        assert!(!manager.apply_delta(&update));

        // Original data unchanged
        let book = manager.book("binance", "BTCUSDT");
        assert_eq!(book.best_bid().unwrap().quantity.raw(), 1_00000000);
        assert_eq!(book.last_update_id(), 100);
    }

    #[test]
    fn test_multi_exchange() {
        let manager = OrderBookManager::new();

        let binance = OrderBookSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 100,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 1_00000000)],
            asks: vec![],
        };

        let kraken = OrderBookSnapshot {
            exchange: "kraken".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 200,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50050_00000000, 2_00000000)],
            asks: vec![],
        };

        manager.apply_snapshot(&binance);
        manager.apply_snapshot(&kraken);

        let binance_book = manager.book("binance", "BTCUSDT");
        let kraken_book = manager.book("kraken", "BTCUSDT");

        assert_eq!(binance_book.best_bid().unwrap().price.raw(), 50000_00000000);
        assert_eq!(kraken_book.best_bid().unwrap().price.raw(), 50050_00000000);

        assert_eq!(manager.symbols().len(), 2);
    }
}
