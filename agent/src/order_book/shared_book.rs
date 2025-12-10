use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use trading_core::{DepthSnapshotEvent, Price, PriceLevel, Quantity};

use crate::gateway_in::{ExchangeId, OrderBookWriter, QualifiedSymbol, StreamData};

/// Multi-symbol order book manager with exchange-qualified keys
/// Thread-safe, can be cloned and shared across threads
#[derive(Clone)]
pub struct OrderBookManager {
    books: Arc<RwLock<HashMap<QualifiedSymbol, OrderBookState>>>,
}

impl OrderBookManager {
    pub fn new() -> Self {
        OrderBookManager {
            books: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create an order book for a qualified symbol
    fn get_or_create(&self, key: &QualifiedSymbol) -> SharedOrderBook {
        // Check if exists first (read lock)
        {
            let books = self.books.read();
            if books.contains_key(key) {
                return SharedOrderBook {
                    manager: self.clone(),
                    key: key.clone(),
                };
            }
        }

        // Create new book (write lock)
        {
            let mut books = self.books.write();
            books.entry(key.clone()).or_insert_with(|| OrderBookState {
                bids: BTreeMap::new(),
                asks: BTreeMap::new(),
                last_update_id: 0,
                initialized: false,
            });
        }

        SharedOrderBook {
            manager: self.clone(),
            key: key.clone(),
        }
    }

    /// Get a handle to a specific order book by exchange and symbol
    pub fn book(
        &self,
        exchange: impl Into<ExchangeId>,
        symbol: impl Into<String>,
    ) -> SharedOrderBook {
        let key = QualifiedSymbol::new(exchange, symbol);
        self.get_or_create(&key)
    }

    /// Get a handle to a specific order book by qualified symbol
    pub fn book_by_key(&self, key: &QualifiedSymbol) -> SharedOrderBook {
        self.get_or_create(key)
    }

    /// Apply a snapshot to a book (internal method)
    fn apply_snapshot_internal(&self, key: &QualifiedSymbol, snapshot: &DepthSnapshotEvent) {
        let mut books = self.books.write();

        let state = books.entry(key.clone()).or_insert_with(|| OrderBookState {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_update_id: 0,
            initialized: false,
        });

        state.bids.clear();
        state.asks.clear();

        for [price, qty] in &snapshot.bids {
            if let (Ok(p), Ok(q)) = (price.parse::<Decimal>(), qty.parse::<Decimal>())
                && !q.is_zero()
            {
                state.bids.insert(p, q);
            }
        }

        for [price, qty] in &snapshot.asks {
            if let (Ok(p), Ok(q)) = (price.parse::<Decimal>(), qty.parse::<Decimal>())
                && !q.is_zero()
            {
                state.asks.insert(p, q);
            }
        }

        state.last_update_id = snapshot.last_update_id;
        state.initialized = true;
    }

    /// Apply a depth update from WebSocket stream (internal method)
    /// Returns true if the update was applied
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
        let mut books = self.books.write();

        let Some(state) = books.get_mut(&key) else {
            return false;
        };

        if !state.initialized {
            return false;
        }

        let expected = state.last_update_id + 1;
        if *first_update_id > expected || *final_update_id < expected {
            return false;
        }

        for [price, qty] in bids {
            if let (Ok(p), Ok(q)) = (price.parse::<Decimal>(), qty.parse::<Decimal>()) {
                if q.is_zero() {
                    state.bids.remove(&p);
                } else {
                    state.bids.insert(p, q);
                }
            }
        }

        for [price, qty] in asks {
            if let (Ok(p), Ok(q)) = (price.parse::<Decimal>(), qty.parse::<Decimal>()) {
                if q.is_zero() {
                    state.asks.remove(&p);
                } else {
                    state.asks.insert(p, q);
                }
            }
        }

        state.last_update_id = *final_update_id;
        true
    }

    /// List all qualified symbols with initialized books
    pub fn symbols(&self) -> Vec<QualifiedSymbol> {
        self.books
            .read()
            .iter()
            .filter(|(_, state)| state.initialized)
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// List all symbols for a specific exchange
    pub fn symbols_for_exchange(&self, exchange_id: &ExchangeId) -> Vec<String> {
        self.books
            .read()
            .iter()
            .filter(|(key, state)| state.initialized && &key.exchange == exchange_id)
            .map(|(key, _)| key.symbol.clone())
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

struct OrderBookState {
    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,
    last_update_id: u64,
    initialized: bool,
}

/// Handle to a single order book within the manager
#[derive(Clone)]
pub struct SharedOrderBook {
    manager: OrderBookManager,
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

    /// Apply a snapshot
    pub fn apply_snapshot(&self, snapshot: &DepthSnapshotEvent) {
        self.manager.apply_snapshot_internal(&self.key, snapshot);
    }

    /// Get the best bid (highest buy price)
    pub fn best_bid(&self) -> Option<PriceLevel> {
        let books = self.manager.books.read();
        let state = books.get(&self.key)?;
        state
            .bids
            .iter()
            .next_back()
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
    }

    /// Get the best ask (lowest sell price)
    pub fn best_ask(&self) -> Option<PriceLevel> {
        let books = self.manager.books.read();
        let state = books.get(&self.key)?;
        state
            .asks
            .iter()
            .next()
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
    }

    /// Get the mid price
    pub fn mid_price(&self) -> Option<Price> {
        let books = self.manager.books.read();
        let state = books.get(&self.key)?;
        let best_bid = state.bids.iter().next_back()?.0;
        let best_ask = state.asks.iter().next()?.0;
        let mid = (*best_bid + *best_ask) / Decimal::TWO;
        Some(Price::from(mid))
    }

    /// Get the spread (best ask - best bid)
    pub fn spread(&self) -> Option<Price> {
        let books = self.manager.books.read();
        let state = books.get(&self.key)?;
        let best_bid = state.bids.iter().next_back()?.0;
        let best_ask = state.asks.iter().next()?.0;
        Some(Price::from(*best_ask - *best_bid))
    }

    /// Get top N bid levels
    pub fn top_bids(&self, n: usize) -> Vec<PriceLevel> {
        let books = self.manager.books.read();
        let Some(state) = books.get(&self.key) else {
            return Vec::new();
        };
        state
            .bids
            .iter()
            .rev()
            .take(n)
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
            .collect()
    }

    /// Get top N ask levels
    pub fn top_asks(&self, n: usize) -> Vec<PriceLevel> {
        let books = self.manager.books.read();
        let Some(state) = books.get(&self.key) else {
            return Vec::new();
        };
        state
            .asks
            .iter()
            .take(n)
            .map(|(p, q)| PriceLevel::new(Price::from(*p), Quantity::from(*q)))
            .collect()
    }

    /// Get the last update ID
    pub fn last_update_id(&self) -> u64 {
        self.manager
            .books
            .read()
            .get(&self.key)
            .map(|s| s.last_update_id)
            .unwrap_or(0)
    }

    /// Check if the book is initialized
    pub fn is_initialized(&self) -> bool {
        self.manager
            .books
            .read()
            .get(&self.key)
            .map(|s| s.initialized)
            .unwrap_or(false)
    }

    /// Get total bid volume up to a price level
    pub fn bid_volume_to_price(&self, price: Price) -> Quantity {
        let books = self.manager.books.read();
        let Some(state) = books.get(&self.key) else {
            return Quantity::ZERO;
        };
        let price_dec = price.inner();
        let total: Decimal = state.bids.range(price_dec..).map(|(_, q)| q).sum();
        Quantity::from(total)
    }

    /// Get total ask volume up to a price level
    pub fn ask_volume_to_price(&self, price: Price) -> Quantity {
        let books = self.manager.books.read();
        let Some(state) = books.get(&self.key) else {
            return Quantity::ZERO;
        };
        let price_dec = price.inner();
        let total: Decimal = state.asks.range(..=price_dec).map(|(_, q)| q).sum();
        Quantity::from(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_symbol_same_exchange() {
        let manager = OrderBookManager::new();
        let binance = ExchangeId::binance();

        // Apply snapshots for two symbols on same exchange
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

        // Get handles to each book
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

        // Same symbol on different exchanges
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
                bids: vec![["50050".to_string(), "2.0".to_string()]], // Different price!
                asks: vec![],
            },
        );

        // Verify they are separate books
        let binance = manager.book("binance", "BTCUSDT");
        let kraken = manager.book("kraken", "BTCUSDT");

        assert_eq!(binance.best_bid().unwrap().price.to_string(), "50000");
        assert_eq!(kraken.best_bid().unwrap().price.to_string(), "50050");

        // Check exchange-specific listing
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
        assert_eq!(btc.best_bid().unwrap().quantity.to_string(), "2.0");
        assert_eq!(btc.last_update_id(), 102);
    }

    #[test]
    fn test_case_insensitive_symbol() {
        let manager = OrderBookManager::new();

        // Apply with lowercase symbol (QualifiedSymbol normalizes to uppercase)
        let key = QualifiedSymbol::new("binance", "btcusdt");
        manager.apply_snapshot_internal(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["50000".to_string(), "1.0".to_string()]],
                asks: vec![],
            },
        );

        // Access with uppercase
        let btc = manager.book("binance", "BTCUSDT");
        assert!(btc.is_initialized());
        assert_eq!(btc.best_bid().unwrap().price.to_string(), "50000");
    }

    #[test]
    fn test_shared_across_handles() {
        let manager = OrderBookManager::new();

        let btc1 = manager.book("binance", "BTCUSDT");
        let btc2 = manager.book("binance", "BTCUSDT");

        btc1.apply_snapshot(&DepthSnapshotEvent {
            last_update_id: 100,
            bids: vec![["50000".to_string(), "1.0".to_string()]],
            asks: vec![],
        });

        // btc2 sees the same data
        assert!(btc2.is_initialized());
        assert_eq!(btc2.best_bid().unwrap().price.to_string(), "50000");
    }
}
