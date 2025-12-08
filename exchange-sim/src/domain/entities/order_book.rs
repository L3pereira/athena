use crate::domain::entities::{Order, PriceLevel, Trade};
use crate::domain::matching::{MatchingAlgorithm, PriceTimeMatcher};
use crate::domain::value_objects::{OrderId, Price, Quantity, Side, Symbol, Timestamp};
use indexmap::IndexMap;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

/// Order book for a single instrument with configurable matching algorithm
#[derive(Clone)]
pub struct OrderBook {
    symbol: Symbol,
    /// Bids sorted by price descending (highest first)
    bids: BTreeMap<PriceKey, VecDeque<Order>>,
    /// Asks sorted by price ascending (lowest first)
    asks: BTreeMap<PriceKey, VecDeque<Order>>,
    /// Quick lookup for orders by ID
    order_index: HashMap<OrderId, (Side, Price)>,
    /// Last update sequence number
    sequence: u64,
    /// Total bid quantity at each price level
    bid_quantities: IndexMap<Price, Quantity>,
    /// Total ask quantity at each price level
    ask_quantities: IndexMap<Price, Quantity>,
    /// Matching algorithm
    matcher: Arc<dyn MatchingAlgorithm>,
}

impl std::fmt::Debug for OrderBook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderBook")
            .field("symbol", &self.symbol)
            .field("bids_count", &self.bids.len())
            .field("asks_count", &self.asks.len())
            .field("order_count", &self.order_index.len())
            .field("sequence", &self.sequence)
            .field("matcher", &self.matcher.name())
            .finish()
    }
}

/// Price key for BTreeMap ordering
/// For bids: negate to sort descending
/// For asks: natural order (ascending)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PriceKey {
    price: Decimal,
    is_bid: bool,
}

impl PriceKey {
    fn bid(price: Price) -> Self {
        PriceKey {
            price: price.inner(),
            is_bid: true,
        }
    }

    fn ask(price: Price) -> Self {
        PriceKey {
            price: price.inner(),
            is_bid: false,
        }
    }
}

impl Ord for PriceKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.is_bid {
            // Bids: higher price first (reverse order)
            other.price.cmp(&self.price)
        } else {
            // Asks: lower price first (natural order)
            self.price.cmp(&other.price)
        }
    }
}

impl PartialOrd for PriceKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl OrderBook {
    /// Create a new order book with default Price-Time Priority matching
    pub fn new(symbol: Symbol) -> Self {
        Self::with_matcher(symbol, Arc::new(PriceTimeMatcher::new()))
    }

    /// Create a new order book with a specific matching algorithm
    pub fn with_matcher(symbol: Symbol, matcher: Arc<dyn MatchingAlgorithm>) -> Self {
        OrderBook {
            symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_index: HashMap::new(),
            sequence: 0,
            bid_quantities: IndexMap::new(),
            ask_quantities: IndexMap::new(),
            matcher,
        }
    }

    /// Get the name of the matching algorithm
    pub fn matcher_name(&self) -> &str {
        self.matcher.name()
    }

    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn increment_sequence(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }

    /// Best bid price (highest buy order)
    pub fn best_bid(&self) -> Option<Price> {
        self.bids
            .first_key_value()
            .map(|(k, _)| Price::from(k.price))
    }

    /// Best ask price (lowest sell order)
    pub fn best_ask(&self) -> Option<Price> {
        self.asks
            .first_key_value()
            .map(|(k, _)| Price::from(k.price))
    }

    /// Mid price between best bid and ask
    pub fn mid_price(&self) -> Option<Price> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => {
                let mid = (bid.inner() + ask.inner()) / Decimal::TWO;
                Some(Price::from(mid))
            }
            _ => None,
        }
    }

    /// Spread between best ask and best bid
    pub fn spread(&self) -> Option<Price> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) if ask > bid => Some(ask - bid),
            _ => None,
        }
    }

    /// Add an order to the book (assumes order is valid and not marketable)
    pub fn add_order(&mut self, order: Order) {
        let price = order.price.expect("Limit order must have price");
        let side = order.side;
        let order_id = order.id;

        match side {
            Side::Buy => {
                let key = PriceKey::bid(price);
                self.bids.entry(key).or_default().push_back(order);
                *self.bid_quantities.entry(price).or_insert(Quantity::ZERO) = self
                    .bid_quantities
                    .get(&price)
                    .copied()
                    .unwrap_or(Quantity::ZERO)
                    + self.bids[&key].back().unwrap().remaining_quantity();
            }
            Side::Sell => {
                let key = PriceKey::ask(price);
                self.asks.entry(key).or_default().push_back(order);
                *self.ask_quantities.entry(price).or_insert(Quantity::ZERO) = self
                    .ask_quantities
                    .get(&price)
                    .copied()
                    .unwrap_or(Quantity::ZERO)
                    + self.asks[&key].back().unwrap().remaining_quantity();
            }
        }

        self.order_index.insert(order_id, (side, price));
        self.increment_sequence();
    }

    /// Remove an order from the book
    pub fn remove_order(&mut self, order_id: OrderId) -> Option<Order> {
        let (side, price) = self.order_index.remove(&order_id)?;

        let order = match side {
            Side::Buy => {
                let key = PriceKey::bid(price);
                let queue = self.bids.get_mut(&key)?;
                let pos = queue.iter().position(|o| o.id == order_id)?;
                let order = queue.remove(pos)?;

                // Update quantity
                if let Some(qty) = self.bid_quantities.get_mut(&price) {
                    *qty = qty.saturating_sub(order.remaining_quantity());
                    if qty.is_zero() {
                        self.bid_quantities.swap_remove(&price);
                    }
                }

                if queue.is_empty() {
                    self.bids.remove(&key);
                }
                order
            }
            Side::Sell => {
                let key = PriceKey::ask(price);
                let queue = self.asks.get_mut(&key)?;
                let pos = queue.iter().position(|o| o.id == order_id)?;
                let order = queue.remove(pos)?;

                // Update quantity
                if let Some(qty) = self.ask_quantities.get_mut(&price) {
                    *qty = qty.saturating_sub(order.remaining_quantity());
                    if qty.is_zero() {
                        self.ask_quantities.swap_remove(&price);
                    }
                }

                if queue.is_empty() {
                    self.asks.remove(&key);
                }
                order
            }
        };

        self.increment_sequence();
        Some(order)
    }

    /// Get an order by ID
    pub fn get_order(&self, order_id: OrderId) -> Option<&Order> {
        let (side, price) = self.order_index.get(&order_id)?;

        match side {
            Side::Buy => {
                let key = PriceKey::bid(*price);
                self.bids.get(&key)?.iter().find(|o| o.id == order_id)
            }
            Side::Sell => {
                let key = PriceKey::ask(*price);
                self.asks.get(&key)?.iter().find(|o| o.id == order_id)
            }
        }
    }

    /// Match an incoming order against the book
    /// Returns trades and the remaining order (if any)
    pub fn match_order(&mut self, mut order: Order, now: Timestamp) -> (Vec<Trade>, Option<Order>) {
        let mut trades = Vec::new();

        loop {
            if order.remaining_quantity().is_zero() {
                break;
            }

            let level_trades = match order.side {
                Side::Buy => self.match_against_asks(&mut order, now),
                Side::Sell => self.match_against_bids(&mut order, now),
            };

            if level_trades.is_empty() {
                break;
            }

            trades.extend(level_trades);
        }

        let remaining = if order.remaining_quantity() > Quantity::ZERO && order.status.is_active() {
            Some(order)
        } else {
            None
        };

        if !trades.is_empty() {
            self.increment_sequence();
        }

        (trades, remaining)
    }

    fn match_against_asks(&mut self, order: &mut Order, now: Timestamp) -> Vec<Trade> {
        let order_price = order.price;

        // Get the best ask key
        let Some((ask_key, _)) = self.asks.first_key_value() else {
            return Vec::new();
        };
        let ask_key = *ask_key;
        let ask_price = Price::from(ask_key.price);

        // Check if order can match (for limit orders, check price)
        if let Some(limit_price) = order_price {
            if limit_price < ask_price {
                return Vec::new();
            }
        }

        let Some(ask_queue) = self.asks.get_mut(&ask_key) else {
            return Vec::new();
        };

        // Use the matching algorithm
        let result = self
            .matcher
            .match_at_level(order, ask_queue, ask_price, now);

        // Update quantity tracking
        let filled_qty: Quantity = result
            .trades
            .iter()
            .map(|t| t.quantity)
            .fold(Quantity::ZERO, |a, b| a + b);

        if let Some(qty) = self.ask_quantities.get_mut(&ask_price) {
            *qty = qty.saturating_sub(filled_qty);
            if qty.is_zero() {
                self.ask_quantities.swap_remove(&ask_price);
            }
        }

        // Remove filled maker orders from index
        for order_id in &result.filled_order_ids {
            self.order_index.remove(order_id);
        }

        // Clean up empty price level
        if let Some(queue) = self.asks.get(&ask_key) {
            if queue.is_empty() {
                self.asks.remove(&ask_key);
            }
        }

        result.trades
    }

    fn match_against_bids(&mut self, order: &mut Order, now: Timestamp) -> Vec<Trade> {
        let order_price = order.price;

        // Get the best bid key
        let Some((bid_key, _)) = self.bids.first_key_value() else {
            return Vec::new();
        };
        let bid_key = *bid_key;
        let bid_price = Price::from(bid_key.price);

        // Check if order can match (for limit orders, check price)
        if let Some(limit_price) = order_price {
            if limit_price > bid_price {
                return Vec::new();
            }
        }

        let Some(bid_queue) = self.bids.get_mut(&bid_key) else {
            return Vec::new();
        };

        // Use the matching algorithm
        let result = self
            .matcher
            .match_at_level(order, bid_queue, bid_price, now);

        // Update quantity tracking
        let filled_qty: Quantity = result
            .trades
            .iter()
            .map(|t| t.quantity)
            .fold(Quantity::ZERO, |a, b| a + b);

        if let Some(qty) = self.bid_quantities.get_mut(&bid_price) {
            *qty = qty.saturating_sub(filled_qty);
            if qty.is_zero() {
                self.bid_quantities.swap_remove(&bid_price);
            }
        }

        // Remove filled maker orders from index
        for order_id in &result.filled_order_ids {
            self.order_index.remove(order_id);
        }

        // Clean up empty price level
        if let Some(queue) = self.bids.get(&bid_key) {
            if queue.is_empty() {
                self.bids.remove(&bid_key);
            }
        }

        result.trades
    }

    /// Get top N bid price levels (sorted descending by price - best bid first)
    pub fn get_bids(&self, depth: usize) -> Vec<PriceLevel> {
        let mut levels: Vec<_> = self
            .bid_quantities
            .iter()
            .map(|(price, qty)| PriceLevel::new(*price, *qty))
            .collect();
        // Sort descending by price (highest first)
        levels.sort_by(|a, b| b.price.cmp(&a.price));
        levels.truncate(depth);
        levels
    }

    /// Get top N ask price levels (sorted ascending by price - best ask first)
    pub fn get_asks(&self, depth: usize) -> Vec<PriceLevel> {
        let mut levels: Vec<_> = self
            .ask_quantities
            .iter()
            .map(|(price, qty)| PriceLevel::new(*price, *qty))
            .collect();
        // Sort ascending by price (lowest first)
        levels.sort_by(|a, b| a.price.cmp(&b.price));
        levels.truncate(depth);
        levels
    }

    /// Get full depth snapshot
    pub fn snapshot(&self, depth: Option<usize>) -> OrderBookSnapshot {
        let depth = depth.unwrap_or(usize::MAX);
        OrderBookSnapshot {
            symbol: self.symbol.clone(),
            bids: self.get_bids(depth),
            asks: self.get_asks(depth),
            sequence: self.sequence,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Number of orders in the book
    pub fn order_count(&self) -> usize {
        self.order_index.len()
    }

    /// Check if book is empty
    pub fn is_empty(&self) -> bool {
        self.order_index.is_empty()
    }
}

/// Immutable snapshot of order book state
#[derive(Debug, Clone, Serialize)]
pub struct OrderBookSnapshot {
    pub symbol: Symbol,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub sequence: u64,
    pub timestamp: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::matching::ProRataMatcher;
    use crate::domain::value_objects::TimeInForce;
    use rust_decimal_macros::dec;

    fn create_symbol() -> Symbol {
        Symbol::new("BTCUSDT").unwrap()
    }

    #[test]
    fn test_add_and_get_order() {
        let mut book = OrderBook::new(create_symbol());
        let order = Order::new_limit(
            create_symbol(),
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );
        let order_id = order.id;

        book.add_order(order);

        assert_eq!(book.order_count(), 1);
        assert!(book.get_order(order_id).is_some());
        assert_eq!(book.best_bid(), Some(Price::from(dec!(100))));
    }

    #[test]
    fn test_match_orders() {
        let mut book = OrderBook::new(create_symbol());

        // Add a sell order
        let sell_order = Order::new_limit(
            create_symbol(),
            Side::Sell,
            Quantity::from(dec!(1)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );
        book.add_order(sell_order);

        // Match with a buy order
        let buy_order = Order::new_limit(
            create_symbol(),
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );

        let now = chrono::Utc::now();
        let (trades, remaining) = book.match_order(buy_order, now);

        assert_eq!(trades.len(), 1);
        assert!(remaining.is_none());
        assert_eq!(book.order_count(), 0);
    }

    #[test]
    fn test_price_time_priority() {
        let mut book = OrderBook::new(create_symbol());

        // Add two sell orders at same price
        let sell1 = Order::new_limit(
            create_symbol(),
            Side::Sell,
            Quantity::from(dec!(1)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );
        let sell1_id = sell1.id;

        let sell2 = Order::new_limit(
            create_symbol(),
            Side::Sell,
            Quantity::from(dec!(1)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );

        book.add_order(sell1);
        book.add_order(sell2);

        // Match - should fill the first order first
        let buy = Order::new_limit(
            create_symbol(),
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );

        let now = chrono::Utc::now();
        let (trades, _) = book.match_order(buy, now);

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].seller_order_id, sell1_id);
    }

    #[test]
    fn test_pro_rata_matching() {
        // Create book with Pro-Rata matcher
        let mut book = OrderBook::with_matcher(create_symbol(), Arc::new(ProRataMatcher::new()));

        assert_eq!(book.matcher_name(), "Pro-Rata");

        // Add two sell orders at same price: 30 and 70 (30% and 70%)
        let sell1 = Order::new_limit(
            create_symbol(),
            Side::Sell,
            Quantity::from(dec!(30)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );

        let sell2 = Order::new_limit(
            create_symbol(),
            Side::Sell,
            Quantity::from(dec!(70)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );

        book.add_order(sell1);
        book.add_order(sell2);

        // Match with buy order for 10
        let buy = Order::new_limit(
            create_symbol(),
            Side::Buy,
            Quantity::from(dec!(10)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );

        let now = chrono::Utc::now();
        let (trades, remaining) = book.match_order(buy, now);

        // Pro-rata: 30% of 10 = 3, 70% of 10 = 7
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].quantity, Quantity::from(dec!(3)));
        assert_eq!(trades[1].quantity, Quantity::from(dec!(7)));
        assert!(remaining.is_none());

        // Both orders should have remaining quantity
        assert_eq!(book.order_count(), 2);
    }
}
