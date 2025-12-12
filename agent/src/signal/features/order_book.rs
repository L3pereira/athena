//! Order book feature extraction
//!
//! Extracts quantitative features from order book data for signal generation.

use crate::order_book::SharedOrderBook;
use crate::signal::domain::Ratio;
use crate::signal::traits::FeatureExtractor;
use std::collections::HashMap;
use trading_core::{PRICE_SCALE, Price, Quantity};

/// Order book feature extractor
///
/// Extracts the following features:
/// - `microprice`: Volume-weighted mid price
/// - `mid_price`: Simple mid price
/// - `spread`: Bid-ask spread
/// - `spread_bps`: Spread in basis points
/// - `imbalance`: Order book imbalance [-1, 1]
/// - `bid_depth`: Total bid depth (configurable levels)
/// - `ask_depth`: Total ask depth (configurable levels)
/// - `depth_ratio`: bid_depth / ask_depth
/// - `top_bid_size`: Size at best bid
/// - `top_ask_size`: Size at best ask
/// - `size_imbalance`: (top_bid - top_ask) / (top_bid + top_ask)
/// - `vwap_bid`: Volume-weighted average bid price
/// - `vwap_ask`: Volume-weighted average ask price
#[derive(Debug, Clone)]
pub struct OrderBookFeatures {
    /// Number of levels to consider for depth calculations
    depth_levels: usize,
}

impl OrderBookFeatures {
    /// Create with default 5 levels
    pub fn new() -> Self {
        Self { depth_levels: 5 }
    }

    /// Create with custom depth levels
    pub fn with_depth(depth_levels: usize) -> Self {
        Self {
            depth_levels: depth_levels.max(1),
        }
    }

    /// Calculate microprice (volume-weighted mid)
    ///
    /// microprice = (bid_price * ask_size + ask_price * bid_size) / (bid_size + ask_size)
    /// Returns a Price type for type safety
    pub fn microprice(
        best_bid: Price,
        best_ask: Price,
        bid_size: Quantity,
        ask_size: Quantity,
    ) -> Option<Price> {
        let total_size = bid_size.raw() as i128 + ask_size.raw() as i128;
        if total_size == 0 {
            return None;
        }
        // microprice = (bid * ask_size + ask * bid_size) / total_size
        let numerator = best_bid.raw() as i128 * ask_size.raw() as i128
            + best_ask.raw() as i128 * bid_size.raw() as i128;
        let microprice_raw = numerator / total_size;
        Some(Price::from_raw(microprice_raw as i64))
    }

    /// Calculate order book imbalance at top of book
    ///
    /// imbalance = (bid_size - ask_size) / (bid_size + ask_size)
    /// Returns a Ratio in [-1, 1]
    pub fn imbalance(bid_size: Quantity, ask_size: Quantity) -> Ratio {
        Ratio::imbalance(bid_size, ask_size)
    }

    /// Calculate VWAP for one side of the book
    /// Returns a Price type representing the volume-weighted average price
    fn vwap(levels: &[(Price, Quantity)], max_levels: usize) -> Option<Price> {
        let mut total_value: i128 = 0;
        let mut total_size: i128 = 0;

        for (price, size) in levels.iter().take(max_levels) {
            total_value += price.raw() as i128 * size.raw() as i128;
            total_size += size.raw() as i128;
        }

        if total_size == 0 {
            None
        } else {
            // VWAP = total_value / total_size, but total_value is price*qty (both scaled by 10^8)
            // So we need to divide by 10^8 once to get back to price scale
            let vwap_raw = total_value / total_size;
            Some(Price::from_raw(vwap_raw as i64))
        }
    }

    /// Calculate total depth for one side
    /// Returns a Quantity representing the total volume across levels
    fn total_depth(levels: &[(Price, Quantity)], max_levels: usize) -> Quantity {
        let sum: i64 = levels
            .iter()
            .take(max_levels)
            .map(|(_, size)| size.raw())
            .sum();
        Quantity::from_raw(sum)
    }

    /// Extract features from a SharedOrderBook
    pub fn extract_from_book(&self, book: &SharedOrderBook) -> HashMap<String, f64> {
        let mut features = HashMap::new();

        // Get top of book
        let best_bid = book.best_bid();
        let best_ask = book.best_ask();

        // If no quotes, return empty features
        let bid_level = match best_bid {
            Some(b) => b,
            None => {
                features.insert("has_bids".to_string(), 0.0);
                return features;
            }
        };

        let ask_level = match best_ask {
            Some(a) => a,
            None => {
                features.insert("has_asks".to_string(), 0.0);
                return features;
            }
        };

        let bid_price = bid_level.price;
        let bid_size = bid_level.quantity;
        let ask_price = ask_level.price;
        let ask_size = ask_level.quantity;

        features.insert("has_bids".to_string(), 1.0);
        features.insert("has_asks".to_string(), 1.0);

        // Mid price
        let mid_raw = (bid_price.raw() + ask_price.raw()) / 2;
        let mid = Price::from_raw(mid_raw);
        features.insert("mid_price".to_string(), mid.to_f64());

        // Spread
        let spread = ask_price.raw() - bid_price.raw();
        features.insert("spread".to_string(), Price::from_raw(spread).to_f64());

        // Spread in basis points
        if mid_raw != 0 {
            let spread_bps = (spread as i128 * 10000 * PRICE_SCALE as i128)
                / mid_raw as i128
                / PRICE_SCALE as i128;
            features.insert("spread_bps".to_string(), spread_bps as f64);
        }

        // Microprice
        if let Some(microprice) = Self::microprice(bid_price, ask_price, bid_size, ask_size) {
            features.insert("microprice".to_string(), microprice.to_f64());

            // Microprice vs mid (indicates direction pressure)
            if mid_raw != 0 {
                let microprice_skew_bps =
                    ((microprice.raw() - mid_raw) as i128 * 10000) / mid_raw as i128;
                features.insert(
                    "microprice_skew_bps".to_string(),
                    microprice_skew_bps as f64,
                );
            }
        }

        // Top of book imbalance
        let imbalance = Self::imbalance(bid_size, ask_size);
        features.insert("imbalance".to_string(), imbalance.value());

        // Top of book sizes
        features.insert("top_bid_size".to_string(), bid_size.to_f64());
        features.insert("top_ask_size".to_string(), ask_size.to_f64());

        // Size imbalance at top level
        let total_top_size = bid_size.raw() + ask_size.raw();
        if total_top_size != 0 {
            let size_imbalance = (bid_size.raw() - ask_size.raw()) as f64 / total_top_size as f64;
            features.insert("size_imbalance".to_string(), size_imbalance);
        }

        // Get depth levels for calculations
        let top_bids = book.top_bids(self.depth_levels);
        let top_asks = book.top_asks(self.depth_levels);

        // Convert to (Price, Quantity) tuples
        let bids: Vec<_> = top_bids.iter().map(|l| (l.price, l.quantity)).collect();
        let asks: Vec<_> = top_asks.iter().map(|l| (l.price, l.quantity)).collect();

        let bid_depth = Self::total_depth(&bids, self.depth_levels);
        let ask_depth = Self::total_depth(&asks, self.depth_levels);

        features.insert("bid_depth".to_string(), bid_depth.to_f64());
        features.insert("ask_depth".to_string(), ask_depth.to_f64());

        // Depth ratio
        if !ask_depth.is_zero() {
            let depth_ratio = bid_depth.raw() as f64 / ask_depth.raw() as f64;
            features.insert("depth_ratio".to_string(), depth_ratio);
        }

        // VWAP for each side
        if let Some(vwap_bid) = Self::vwap(&bids, self.depth_levels) {
            features.insert("vwap_bid".to_string(), vwap_bid.to_f64());
        }
        if let Some(vwap_ask) = Self::vwap(&asks, self.depth_levels) {
            features.insert("vwap_ask".to_string(), vwap_ask.to_f64());
        }

        // Book pressure: weighted imbalance across multiple levels
        let total_depth = bid_depth.raw() + ask_depth.raw();
        if total_depth != 0 {
            let book_pressure = (bid_depth.raw() - ask_depth.raw()) as f64 / total_depth as f64;
            features.insert("book_pressure".to_string(), book_pressure);
        }

        features
    }
}

impl Default for OrderBookFeatures {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureExtractor for OrderBookFeatures {
    fn extract(&self, book: &SharedOrderBook) -> HashMap<String, f64> {
        self.extract_from_book(book)
    }

    fn feature_names(&self) -> &[&str] {
        &[
            "has_bids",
            "has_asks",
            "mid_price",
            "spread",
            "spread_bps",
            "microprice",
            "microprice_skew_bps",
            "imbalance",
            "top_bid_size",
            "top_ask_size",
            "size_imbalance",
            "bid_depth",
            "ask_depth",
            "depth_ratio",
            "vwap_bid",
            "vwap_ask",
            "book_pressure",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway_in::{OrderBookWriter, QualifiedSymbol};
    use crate::order_book::OrderBookManager;
    use trading_core::DepthSnapshotEvent;

    fn create_test_book() -> (OrderBookManager, QualifiedSymbol) {
        let manager = OrderBookManager::new();
        let key = QualifiedSymbol::new("binance", "BTCUSDT");

        manager.apply_snapshot(
            &key,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![
                    ["100".to_string(), "10".to_string()], // Best bid
                    ["99".to_string(), "20".to_string()],
                    ["98".to_string(), "30".to_string()],
                ],
                asks: vec![
                    ["101".to_string(), "5".to_string()], // Best ask
                    ["102".to_string(), "15".to_string()],
                    ["103".to_string(), "25".to_string()],
                ],
            },
        );

        (manager, key)
    }

    #[test]
    fn test_microprice() {
        // bid=100 size=10, ask=101 size=5
        // microprice = (100*5 + 101*10) / (10+5) = (500 + 1010) / 15 = 100.666...
        let mp = OrderBookFeatures::microprice(
            Price::from_int(100),
            Price::from_int(101),
            Quantity::from_int(10),
            Quantity::from_int(5),
        )
        .unwrap();

        // With more bid size, microprice should be closer to ask (pressure to buy)
        assert!(mp.to_f64() > 100.0);
        assert!(mp.to_f64() < 101.0);
    }

    #[test]
    fn test_imbalance() {
        // More bids than asks -> positive imbalance
        let imb = OrderBookFeatures::imbalance(Quantity::from_int(10), Quantity::from_int(5));
        assert!(imb.value() > 0.0);
        assert!((imb.value() - 0.333).abs() < 0.01); // (10-5)/(10+5) = 5/15 ≈ 0.333

        // More asks than bids -> negative imbalance
        let imb = OrderBookFeatures::imbalance(Quantity::from_int(5), Quantity::from_int(10));
        assert!(imb.value() < 0.0);

        // Equal -> zero
        let imb = OrderBookFeatures::imbalance(Quantity::from_int(10), Quantity::from_int(10));
        assert_eq!(imb.value(), 0.0);
    }

    #[test]
    fn test_extract_features() {
        let (manager, key) = create_test_book();
        let book = manager.book_by_key(&key);
        let extractor = OrderBookFeatures::new();

        let features = extractor.extract(&book);

        // Check basic features exist
        assert_eq!(features.get("has_bids"), Some(&1.0));
        assert_eq!(features.get("has_asks"), Some(&1.0));

        // Mid price should be (100 + 101) / 2 = 100.5
        let mid = features.get("mid_price").unwrap();
        assert!((mid - 100.5).abs() < 0.01);

        // Spread should be 101 - 100 = 1
        let spread = features.get("spread").unwrap();
        assert!((spread - 1.0).abs() < 0.01);

        // Imbalance: bid=10, ask=5 -> (10-5)/(10+5) ≈ 0.333
        let imbalance = features.get("imbalance").unwrap();
        assert!(imbalance > &0.0);

        // Bid depth: 10 + 20 + 30 = 60 (for 5 levels, we only have 3)
        let bid_depth = features.get("bid_depth").unwrap();
        assert!((bid_depth - 60.0).abs() < 0.01);

        // Ask depth: 5 + 15 + 25 = 45
        let ask_depth = features.get("ask_depth").unwrap();
        assert!((ask_depth - 45.0).abs() < 0.01);
    }

    #[test]
    fn test_empty_book() {
        let manager = OrderBookManager::new();
        let key = QualifiedSymbol::new("binance", "BTCUSDT");

        // Get book without initializing it
        let book = manager.book_by_key(&key);
        let extractor = OrderBookFeatures::new();

        let features = extractor.extract(&book);

        // Should have has_bids = 0 and minimal features
        assert_eq!(features.get("has_bids"), Some(&0.0));
    }
}
