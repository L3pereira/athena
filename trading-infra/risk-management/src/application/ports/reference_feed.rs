//! Reference Feed Port
//!
//! Trait for accessing reference market data (e.g., Binance) for regime detection
//! and market making decisions.

use crate::domain::OrderbookMoments;
use trading_core::Price;

/// A tick from the reference feed
#[derive(Debug, Clone, Copy)]
pub struct ReferenceTick {
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Reference mid price
    pub mid_price: Price,
    /// Orderbook moments
    pub moments: OrderbookMoments,
}

/// Port for accessing reference market data
///
/// This trait abstracts the source of reference market data (Binance, mock, replay).
/// Agents use this to get volatility estimates and reference prices for their models.
pub trait ReferenceFeed: Send + Sync {
    /// Get current orderbook moments from reference market
    fn moments(&self) -> OrderbookMoments;

    /// Get current mid price from reference market
    fn mid_price(&self) -> Price;

    /// Get current volatility estimate (annualized)
    fn volatility(&self) -> f64 {
        self.moments().mid_volatility
    }

    /// Get current spread in basis points
    fn spread_bps(&self) -> f64 {
        self.moments().spread_bps
    }

    /// Get current depth ratio (depth at best / total depth)
    fn depth_ratio(&self) -> f64 {
        self.moments().depth_ratio
    }

    /// Get current imbalance (-1 to 1, negative = more sells)
    fn imbalance(&self) -> f64 {
        self.moments().imbalance
    }

    /// Check if reference market is in stressed state
    fn is_stressed(&self) -> bool {
        self.moments().is_stressed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockFeed {
        moments: OrderbookMoments,
        mid_price: Price,
    }

    impl ReferenceFeed for MockFeed {
        fn moments(&self) -> OrderbookMoments {
            self.moments.clone()
        }

        fn mid_price(&self) -> Price {
            self.mid_price
        }
    }

    #[test]
    fn test_reference_feed_trait() {
        let feed = MockFeed {
            moments: OrderbookMoments {
                spread_bps: 10.0,
                depth_ratio: 0.8,
                imbalance: 0.1,
                mid_volatility: 0.02,
                ..Default::default()
            },
            mid_price: Price::from_int(50000),
        };

        assert_eq!(feed.spread_bps(), 10.0);
        assert_eq!(feed.depth_ratio(), 0.8);
        assert_eq!(feed.imbalance(), 0.1);
        assert_eq!(feed.volatility(), 0.02);
        assert!(!feed.is_stressed());
        assert_eq!(feed.mid_price(), Price::from_int(50000));
    }

    #[test]
    fn test_stressed_detection() {
        let feed = MockFeed {
            moments: OrderbookMoments {
                spread_bps: 100.0, // > 50 bps = stressed
                depth_ratio: 0.2,  // < 0.3 = stressed
                imbalance: 0.1,
                mid_volatility: 0.02,
                ..Default::default()
            },
            mid_price: Price::from_int(50000),
        };

        assert!(feed.is_stressed());
    }
}
