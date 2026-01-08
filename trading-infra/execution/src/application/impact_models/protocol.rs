//! Impact Model Protocol
//!
//! Core trait for impact estimation (SOLID: OCP)

use crate::domain::{FullImpact, Impact, MarketState};
use trading_core::{Quantity, Side};

/// Impact estimation interface (Open for extension, closed for modification)
///
/// Implementations estimate the market impact of a potential order.
/// All implementations must be thread-safe (Send + Sync).
pub trait ImpactModel: Send + Sync {
    /// Estimate simple price impact for an order
    ///
    /// # Arguments
    /// * `order_size` - The size of the order
    /// * `side` - Buy or Sell
    /// * `market` - Current market state
    ///
    /// # Returns
    /// Estimated impact in basis points
    fn estimate(&self, order_size: Quantity, side: Side, market: &MarketState) -> Impact;

    /// Estimate full L2 structure impact (optional, default returns zero)
    ///
    /// Provides multi-dimensional impact: price, spread, depth, volatility, regime
    fn estimate_full(&self, order_size: Quantity, side: Side, market: &MarketState) -> FullImpact {
        let simple = self.estimate(order_size, side, market);
        FullImpact {
            price_impact_bps: simple.price_bps,
            ..Default::default()
        }
    }

    /// Get the model name for logging/debugging
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Price;

    struct TestImpact;

    impl ImpactModel for TestImpact {
        fn estimate(&self, _: Quantity, _: Side, _: &MarketState) -> Impact {
            Impact::permanent(5.0)
        }

        fn name(&self) -> &str {
            "test"
        }
    }

    #[test]
    fn test_trait_object() {
        let model: Box<dyn ImpactModel> = Box::new(TestImpact);
        let market = MarketState {
            mid_price: Price::from_int(100),
            ..Default::default()
        };
        let impact = model.estimate(Quantity::from_int(100), Side::Buy, &market);
        assert!((impact.price_bps - 5.0).abs() < 0.001);
    }
}
