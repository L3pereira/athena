//! Order Book Feature Extractor - Infrastructure implementation
//!
//! Implements FeatureExtractorPort using domain calculations and OrderBookReader.
//! Uses the clean architecture pattern: depends on abstractions, not concretes.

use crate::signal::application::ports::{FeatureExtractorPort, OrderBookReader};
use crate::signal::domain::{Calculations, Features, Imbalance, Microprice, Spread, Vwap};
// Types imported for test mocking only
#[cfg(test)]
use trading_core::{Price, Quantity};

/// Order book feature extractor using domain calculations
///
/// This extractor:
/// - Implements the FeatureExtractorPort trait
/// - Uses the OrderBookReader abstraction (not concrete SharedOrderBook)
/// - Delegates calculations to domain layer
#[derive(Debug, Clone)]
pub struct OrderBookExtractor {
    /// Number of levels to consider for depth calculations
    depth_levels: usize,
}

impl OrderBookExtractor {
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
}

impl Default for OrderBookExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureExtractorPort for OrderBookExtractor {
    fn extract(&self, book: &dyn OrderBookReader) -> Features {
        let mut features = Features::new();

        if !book.is_initialized() {
            features.set("initialized", 0.0);
            return features;
        }

        // Get top of book
        let best_bid = book.best_bid();
        let best_ask = book.best_ask();

        let (bid_price, bid_size) = match &best_bid {
            Some(level) => (level.price, level.size),
            None => {
                features.set("has_bids", 0.0);
                return features;
            }
        };

        let (ask_price, ask_size) = match &best_ask {
            Some(level) => (level.price, level.size),
            None => {
                features.set("has_asks", 0.0);
                return features;
            }
        };

        features.set("has_bids", 1.0);
        features.set("has_asks", 1.0);
        features.set("initialized", 1.0);

        // Use domain calculations for spread
        let spread = Spread::new(bid_price, ask_price);
        features.set("spread", spread.calculate().to_f64());
        features.set("mid_price", spread.mid_price().to_f64());

        if let Some(bps) = spread.bps() {
            features.set_bps("spread_bps", bps);
        }

        // Use domain calculations for microprice
        let microprice = Microprice::new(bid_price, ask_price, bid_size, ask_size);
        if let Some(mp) = Calculations::microprice(microprice.clone()) {
            features.set("microprice", mp.to_f64());
        }
        if let Some(skew) = microprice.skew_bps() {
            features.set_bps("microprice_skew_bps", skew);
        }

        // Use domain calculations for imbalance
        let imbalance = Imbalance::new(bid_size, ask_size);
        features.set_ratio("imbalance", Calculations::imbalance(imbalance.clone()));
        features.set(
            "is_buying_pressure",
            if imbalance.is_buying_pressure() {
                1.0
            } else {
                0.0
            },
        );

        // Top of book sizes
        features.set("top_bid_size", bid_size.to_f64());
        features.set("top_ask_size", ask_size.to_f64());

        // Get depth levels for calculations
        let bid_levels = book.bid_levels(self.depth_levels);
        let ask_levels = book.ask_levels(self.depth_levels);

        // Total depth
        let bid_depth = book.total_bid_depth(self.depth_levels);
        let ask_depth = book.total_ask_depth(self.depth_levels);

        features.set("bid_depth", bid_depth.to_f64());
        features.set("ask_depth", ask_depth.to_f64());

        // Depth ratio
        if !ask_depth.is_zero() {
            let depth_ratio = bid_depth.raw() as f64 / ask_depth.raw() as f64;
            features.set("depth_ratio", depth_ratio);
        }

        // Book pressure (depth imbalance)
        let total_depth_raw = bid_depth.raw() + ask_depth.raw();
        if total_depth_raw != 0 {
            let book_pressure = (bid_depth.raw() - ask_depth.raw()) as f64 / total_depth_raw as f64;
            features.set("book_pressure", book_pressure);
        }

        // Use domain VWAP calculation
        let mut bid_vwap = Vwap::new();
        for level in &bid_levels {
            bid_vwap.add_level(level.price, level.size);
        }
        if let Some(vwap_bid) = bid_vwap.calculate() {
            features.set("vwap_bid", vwap_bid.to_f64());
        }

        let mut ask_vwap = Vwap::new();
        for level in &ask_levels {
            ask_vwap.add_level(level.price, level.size);
        }
        if let Some(vwap_ask) = ask_vwap.calculate() {
            features.set("vwap_ask", vwap_ask.to_f64());
        }

        features
    }

    fn feature_names(&self) -> Vec<&'static str> {
        vec![
            "initialized",
            "has_bids",
            "has_asks",
            "mid_price",
            "spread",
            "spread_bps",
            "microprice",
            "microprice_skew_bps",
            "imbalance",
            "is_buying_pressure",
            "top_bid_size",
            "top_ask_size",
            "bid_depth",
            "ask_depth",
            "depth_ratio",
            "book_pressure",
            "vwap_bid",
            "vwap_ask",
        ]
    }

    fn name(&self) -> &'static str {
        "order_book"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::application::ports::BookLevel;

    /// Mock order book for testing
    struct MockOrderBook {
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
        initialized: bool,
    }

    impl MockOrderBook {
        fn new(bids: Vec<(i64, i64)>, asks: Vec<(i64, i64)>) -> Self {
            Self {
                bids: bids
                    .into_iter()
                    .map(|(p, s)| BookLevel {
                        price: Price::from_int(p),
                        size: Quantity::from_int(s),
                    })
                    .collect(),
                asks: asks
                    .into_iter()
                    .map(|(p, s)| BookLevel {
                        price: Price::from_int(p),
                        size: Quantity::from_int(s),
                    })
                    .collect(),
                initialized: true,
            }
        }
    }

    impl OrderBookReader for MockOrderBook {
        fn is_initialized(&self) -> bool {
            self.initialized
        }

        fn best_bid(&self) -> Option<BookLevel> {
            self.bids.first().cloned()
        }

        fn best_ask(&self) -> Option<BookLevel> {
            self.asks.first().cloned()
        }

        fn mid_price(&self) -> Option<Price> {
            let bid = self.bids.first()?;
            let ask = self.asks.first()?;
            Some(Price::from_raw((bid.price.raw() + ask.price.raw()) / 2))
        }

        fn spread(&self) -> Option<Price> {
            let bid = self.bids.first()?;
            let ask = self.asks.first()?;
            Some(Price::from_raw(ask.price.raw() - bid.price.raw()))
        }

        fn bid_levels(&self, depth: usize) -> Vec<BookLevel> {
            self.bids.iter().take(depth).cloned().collect()
        }

        fn ask_levels(&self, depth: usize) -> Vec<BookLevel> {
            self.asks.iter().take(depth).cloned().collect()
        }

        fn total_bid_depth(&self, levels: usize) -> Quantity {
            let sum: i64 = self.bids.iter().take(levels).map(|l| l.size.raw()).sum();
            Quantity::from_raw(sum)
        }

        fn total_ask_depth(&self, levels: usize) -> Quantity {
            let sum: i64 = self.asks.iter().take(levels).map(|l| l.size.raw()).sum();
            Quantity::from_raw(sum)
        }

        fn last_update_time(&self) -> Option<u64> {
            Some(0)
        }
    }

    #[test]
    fn test_extract_features() {
        let book = MockOrderBook::new(
            vec![(100, 10), (99, 20), (98, 30)],
            vec![(101, 5), (102, 15), (103, 25)],
        );

        let extractor = OrderBookExtractor::new();
        let features = extractor.extract(&book);

        // Check basic features
        assert_eq!(features.get("has_bids"), Some(1.0));
        assert_eq!(features.get("has_asks"), Some(1.0));

        // Mid price = (100 + 101) / 2 = 100.5
        let mid = features.get("mid_price").unwrap();
        assert!((mid - 100.5).abs() < 0.01);

        // Spread = 101 - 100 = 1
        let spread = features.get("spread").unwrap();
        assert!((spread - 1.0).abs() < 0.01);

        // Imbalance: bid=10, ask=5 -> (10-5)/(10+5) ≈ 0.333
        let imbalance = features.get_ratio("imbalance").unwrap();
        assert!(imbalance.value() > 0.0);
        assert!((imbalance.value() - 0.333).abs() < 0.01);

        // Microprice should exist and be between bid and ask
        let microprice = features.get("microprice").unwrap();
        assert!(microprice > 100.0);
        assert!(microprice < 101.0);
    }

    #[test]
    fn test_uninitialized_book() {
        let mut book = MockOrderBook::new(vec![], vec![]);
        book.initialized = false;

        let extractor = OrderBookExtractor::new();
        let features = extractor.extract(&book);

        assert_eq!(features.get("initialized"), Some(0.0));
    }

    #[test]
    fn test_feature_names() {
        let extractor = OrderBookExtractor::new();
        let names = extractor.feature_names();

        assert!(names.contains(&"mid_price"));
        assert!(names.contains(&"spread"));
        assert!(names.contains(&"microprice"));
        assert!(names.contains(&"imbalance"));
    }
}
