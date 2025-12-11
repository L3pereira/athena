use arc_swap::ArcSwap;
use dashmap::DashMap;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::sync::Arc;
use trading_core::{DepthSnapshotEvent, Price, PriceLevel, Quantity};

use crate::gateway_in::{ExchangeId, OrderBookWriter, QualifiedSymbol, StreamData};

/// Order book state - immutable once created (copy-on-write)
#[derive(Clone, Debug, Default)]
struct OrderBookState {
    bids: BTreeMap<Decimal, Decimal>, // price -> quantity (descending for best bid)
    asks: BTreeMap<Decimal, Decimal>, // price -> quantity (ascending for best ask)
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

    /// Apply a snapshot (copy-on-write)
    fn apply_snapshot_internal(&self, key: &QualifiedSymbol, snapshot: &DepthSnapshotEvent) {
        let swap = self.get_or_create_swap(key);

        // Build new state
        let mut new_state = OrderBookState {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_update_id: snapshot.last_update_id,
            initialized: true,
        };

        for [price, qty] in &snapshot.bids {
            if let (Ok(p), Ok(q)) = (price.parse::<Decimal>(), qty.parse::<Decimal>())
                && !q.is_zero()
            {
                new_state.bids.insert(p, q);
            }
        }

        for [price, qty] in &snapshot.asks {
            if let (Ok(p), Ok(q)) = (price.parse::<Decimal>(), qty.parse::<Decimal>())
                && !q.is_zero()
            {
                new_state.asks.insert(p, q);
            }
        }

        // Atomic swap - readers see old or new, never partial
        swap.store(Arc::new(new_state));
    }

    /// Apply a delta update (copy-on-write)
    /// Returns true if update was applied successfully
    fn apply_update_internal(&self, exchange_id: &ExchangeId, update: &StreamData) -> bool {
        let StreamData::DepthUpdate {
            symbol,
            first_update_id,
            final_update_id,
            bids,
            asks,
            ..
        } = update
        else {
            return false;
        };

        let key = QualifiedSymbol::new(exchange_id.clone(), symbol);

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
        if *first_update_id > expected || *final_update_id < expected {
            return false;
        }

        // Clone and modify (copy-on-write)
        let mut new_state = (**current).clone();

        for [price, qty] in bids {
            if let (Ok(p), Ok(q)) = (price.parse::<Decimal>(), qty.parse::<Decimal>()) {
                if q.is_zero() {
                    new_state.bids.remove(&p);
                } else {
                    new_state.bids.insert(p, q);
                }
            }
        }

        for [price, qty] in asks {
            if let (Ok(p), Ok(q)) = (price.parse::<Decimal>(), qty.parse::<Decimal>()) {
                if q.is_zero() {
                    new_state.asks.remove(&p);
                } else {
                    new_state.asks.insert(p, q);
                }
            }
        }

        new_state.last_update_id = *final_update_id;

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

/// Implement OrderBookWriter trait (Dependency Inversion)
impl OrderBookWriter for OrderBookManager {
    fn apply_snapshot(&self, key: &QualifiedSymbol, snapshot: &DepthSnapshotEvent) {
        self.apply_snapshot_internal(key, snapshot)
    }

    fn apply_update(&self, exchange_id: &ExchangeId, update: &StreamData) -> bool {
        self.apply_update_internal(exchange_id, update)
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
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
    }

    /// Get the best ask (lowest sell price) - lock-free
    pub fn best_ask(&self) -> Option<PriceLevel> {
        let state = self.load();
        state
            .asks
            .iter()
            .next()
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
    }

    /// Get the mid price - lock-free
    pub fn mid_price(&self) -> Option<Price> {
        let state = self.load();
        let best_bid = state.bids.iter().next_back()?.0;
        let best_ask = state.asks.iter().next()?.0;
        let mid = (*best_bid + *best_ask) / Decimal::TWO;
        Some(Price::from(mid))
    }

    /// Get the spread (best ask - best bid) - lock-free
    pub fn spread(&self) -> Option<Price> {
        let state = self.load();
        let best_bid = state.bids.iter().next_back()?.0;
        let best_ask = state.asks.iter().next()?.0;
        Some(Price::from(*best_ask - *best_bid))
    }

    /// Get top N bid levels - lock-free
    pub fn top_bids(&self, n: usize) -> Vec<PriceLevel> {
        let state = self.load();
        state
            .bids
            .iter()
            .rev()
            .take(n)
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
            .collect()
    }

    /// Get top N ask levels - lock-free
    pub fn top_asks(&self, n: usize) -> Vec<PriceLevel> {
        let state = self.load();
        state
            .asks
            .iter()
            .take(n)
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
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
        let price_dec = price.inner();
        let total: Decimal = state.bids.range(price_dec..).map(|(_, q)| q).sum();
        Quantity::from(total)
    }

    /// Get total ask volume up to a price level - lock-free
    pub fn ask_volume_to_price(&self, price: Price) -> Quantity {
        let state = self.load();
        let price_dec = price.inner();
        let total: Decimal = state.asks.range(..=price_dec).map(|(_, q)| q).sum();
        Quantity::from(total)
    }

    /// Get a consistent snapshot of bids and asks - lock-free
    /// Useful when you need both sides atomically
    pub fn snapshot(&self) -> (Vec<PriceLevel>, Vec<PriceLevel>) {
        let state = self.load();
        let bids = state
            .bids
            .iter()
            .rev()
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
            .collect();
        let asks = state
            .asks
            .iter()
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
            .collect();
        (bids, asks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_free_reads() {
        let manager = OrderBookManager::new();
        let key = QualifiedSymbol::new("binance", "BTCUSDT");

        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![["50100".to_string(), "1.5".to_string()]],
            },
        );

        let book = manager.book("binance", "BTCUSDT");

        // Multiple reads don't block each other
        let bid1 = book.best_bid();
        let bid2 = book.best_bid();
        let ask = book.best_ask();

        assert_eq!(bid1.unwrap().price.to_string(), "50000");
        assert_eq!(bid2.unwrap().price.to_string(), "50000");
        assert_eq!(ask.unwrap().price.to_string(), "50100");
    }

    #[test]
    fn test_atomic_snapshot() {
        let manager = OrderBookManager::new();
        let key = QualifiedSymbol::new("binance", "BTCUSDT");

        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![
                    ["50000".to_string(), "1.0".to_string()],
                    ["49900".to_string(), "2.0".to_string()],
                ],
                asks: vec![
                    ["50100".to_string(), "1.5".to_string()],
                    ["50200".to_string(), "2.5".to_string()],
                ],
            },
        );

        let book = manager.book("binance", "BTCUSDT");

        // Get consistent snapshot of both sides
        let (bids, asks) = book.snapshot();

        assert_eq!(bids.len(), 2);
        assert_eq!(asks.len(), 2);
        assert_eq!(bids[0].price.to_string(), "50000"); // Best bid first
        assert_eq!(asks[0].price.to_string(), "50100"); // Best ask first
    }

    #[test]
    fn test_multi_symbol_same_exchange() {
        let manager = OrderBookManager::new();
        let binance = ExchangeId::binance();

        let btc_key = QualifiedSymbol::new(binance.clone(), "BTCUSDT");
        let eth_key = QualifiedSymbol::new(binance.clone(), "ETHUSDT");

        manager.apply_snapshot_internal(
            &btc_key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![["50100".to_string(), "1.5".to_string()]],
            },
        );

        manager.apply_snapshot_internal(
            &eth_key,
            &DepthSnapshotEvent {
                last_update_id: 200,
                bids: vec![["3000".to_string(), "10.0".to_string()]],
                asks: vec![["3010".to_string(), "15.0".to_string()]],
            },
        );

        let btc = manager.book("binance", "BTCUSDT");
        let eth = manager.book("binance", "ETHUSDT");

        assert_eq!(btc.best_bid().unwrap().price.to_string(), "50000");
        assert_eq!(eth.best_bid().unwrap().price.to_string(), "3000");

        assert_eq!(manager.symbols().len(), 2);
        assert_eq!(manager.symbols_for_exchange(&binance).len(), 2);
    }

    #[test]
    fn test_same_symbol_different_exchanges() {
        let manager = OrderBookManager::new();

        let binance_btc = QualifiedSymbol::new("binance", "BTCUSDT");
        let kraken_btc = QualifiedSymbol::new("kraken", "BTCUSDT");

        manager.apply_snapshot_internal(
            &binance_btc,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![],
            },
        );

        manager.apply_snapshot_internal(
            &kraken_btc,
            &DepthSnapshotEvent {
                last_update_id: 200,
                bids: vec![["50050".to_string(), "2.0".to_string()]],
                asks: vec![],
            },
        );

        let binance = manager.book("binance", "BTCUSDT");
        let kraken = manager.book("kraken", "BTCUSDT");

        assert_eq!(binance.best_bid().unwrap().price.to_string(), "50000");
        assert_eq!(kraken.best_bid().unwrap().price.to_string(), "50050");

        assert_eq!(
            manager.symbols_for_exchange(&ExchangeId::binance()).len(),
            1
        );
        assert_eq!(manager.symbols_for_exchange(&ExchangeId::kraken()).len(), 1);
        assert_eq!(manager.symbols().len(), 2);
    }

    #[test]
    fn test_apply_update_with_exchange() {
        let manager = OrderBookManager::new();
        let exchange = ExchangeId::binance();

        let key = QualifiedSymbol::new(exchange.clone(), "BTCUSDT");
        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![["50100".to_string(), "1.5".to_string()]],
            },
        );

        let update = StreamData::DepthUpdate {
            symbol: "BTCUSDT".to_string(),
            event_time: 0,
            first_update_id: 101,
            final_update_id: 102,
            bids: vec![["50000".to_string(), "2.0".to_string()]],
            asks: vec![],
        };

        assert!(manager.apply_update_internal(&exchange, &update));

        let btc = manager.book("binance", "BTCUSDT");
        assert_eq!(btc.best_bid().unwrap().quantity.inner(), Decimal::from(2));
        assert_eq!(btc.last_update_id(), 102);
    }

    #[test]
    fn test_update_removes_zero_quantity() {
        let manager = OrderBookManager::new();
        let exchange = ExchangeId::binance();

        let key = QualifiedSymbol::new(exchange.clone(), "BTCUSDT");
        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![
                    ["50000".to_string(), "1.0".to_string()],
                    ["49900".to_string(), "2.0".to_string()],
                ],
                asks: vec![],
            },
        );

        // Remove the best bid by setting quantity to 0
        let update = StreamData::DepthUpdate {
            symbol: "BTCUSDT".to_string(),
            event_time: 0,
            first_update_id: 101,
            final_update_id: 101,
            bids: vec![["50000".to_string(), "0".to_string()]],
            asks: vec![],
        };

        assert!(manager.apply_update_internal(&exchange, &update));

        let btc = manager.book("binance", "BTCUSDT");
        // Best bid should now be 49900
        assert_eq!(btc.best_bid().unwrap().price.to_string(), "49900");
    }

    #[test]
    fn test_case_insensitive_symbol() {
        let manager = OrderBookManager::new();

        let key = QualifiedSymbol::new("binance", "btcusdt");
        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![],
            },
        );

        let btc = manager.book("binance", "BTCUSDT");
        assert!(btc.is_initialized());
        assert_eq!(btc.best_bid().unwrap().price.to_string(), "50000");
    }

    #[test]
    fn test_shared_across_handles() {
        let manager = OrderBookManager::new();

        let btc1 = manager.book("binance", "BTCUSDT");
        let btc2 = manager.book("binance", "BTCUSDT");

        // Apply via manager
        let key = QualifiedSymbol::new("binance", "BTCUSDT");
        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![],
            },
        );

        // Both handles see the same data
        assert!(btc1.is_initialized());
        assert!(btc2.is_initialized());
        assert_eq!(btc1.best_bid().unwrap().price.to_string(), "50000");
        assert_eq!(btc2.best_bid().unwrap().price.to_string(), "50000");
    }

    #[test]
    fn test_out_of_sequence_update_rejected() {
        let manager = OrderBookManager::new();
        let exchange = ExchangeId::binance();

        let key = QualifiedSymbol::new(exchange.clone(), "BTCUSDT");
        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![],
            },
        );

        // Gap in sequence (expected 101, got 105)
        let update = StreamData::DepthUpdate {
            symbol: "BTCUSDT".to_string(),
            event_time: 0,
            first_update_id: 105,
            final_update_id: 106,
            bids: vec![["50000".to_string(), "2.0".to_string()]],
            asks: vec![],
        };

        assert!(!manager.apply_update_internal(&exchange, &update));

        // Original data unchanged
        let btc = manager.book("binance", "BTCUSDT");
        assert_eq!(btc.best_bid().unwrap().quantity.inner(), Decimal::from(1));
        assert_eq!(btc.last_update_id(), 100);
    }

    #[test]
    fn test_concurrent_access_simulation() {
        use std::thread;

        let manager = OrderBookManager::new();
        let key = QualifiedSymbol::new("binance", "BTCUSDT");

        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![["50100".to_string(), "1.0".to_string()]],
            },
        );

        let manager_clone = manager.clone();

        // Spawn reader threads
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let m = manager_clone.clone();
                thread::spawn(move || {
                    for _ in 0..1000 {
                        let book = m.book("binance", "BTCUSDT");
                        let _ = book.best_bid();
                        let _ = book.best_ask();
                        let _ = book.spread();
                    }
                })
            })
            .collect();

        // Writer thread
        let writer = {
            let m = manager.clone();
            let ex = ExchangeId::binance();
            thread::spawn(move || {
                for i in 101..200u64 {
                    let update = StreamData::DepthUpdate {
                        symbol: "BTCUSDT".to_string(),
                        event_time: 0,
                        first_update_id: i,
                        final_update_id: i,
                        bids: vec![["50000".to_string(), format!("{}.0", i)]],
                        asks: vec![],
                    };
                    m.apply_update_internal(&ex, &update);
                }
            })
        };

        // All threads complete without deadlock
        for h in handles {
            h.join().unwrap();
        }
        writer.join().unwrap();

        // Final state is consistent
        let book = manager.book("binance", "BTCUSDT");
        assert!(book.is_initialized());
        assert_eq!(book.last_update_id(), 199);
    }
}
