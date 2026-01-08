//! Linear Impact Model
//!
//! The simplest model: impact is proportional to size.
//!
//! Impact = λ × Q
//!
//! Where:
//! - λ = Kyle's lambda (price sensitivity per unit)
//! - Q = order size
//!
//! From Kyle (1985): λ = σ_v / σ_u
//! - σ_v = information variance
//! - σ_u = noise variance
//!
//! Best for: Quick estimates, small orders, liquid markets

use super::protocol::ImpactModel;
use crate::domain::{Impact, MarketState};
use trading_core::{Quantity, Side};

/// Linear impact model using Kyle's lambda
pub struct LinearImpact {
    /// Kyle's lambda: price sensitivity per unit of order size
    /// Typical values: 0.001 - 0.1 bps per unit
    lambda: f64,
}

impl LinearImpact {
    /// Create a new linear impact model
    ///
    /// # Arguments
    /// * `lambda` - Kyle's lambda (impact bps per unit of quantity)
    pub fn new(lambda: f64) -> Self {
        Self { lambda }
    }

    /// Create with estimated lambda from market data
    ///
    /// Rough estimate: λ ≈ spread_bps / (2 × depth)
    pub fn from_market(market: &MarketState) -> Self {
        let depth = market.total_depth().to_f64();
        if depth <= 0.0 {
            return Self::new(0.01); // Default fallback
        }
        let spread_bps = market.spread_bps();
        let lambda = spread_bps / (2.0 * depth);
        Self::new(lambda.max(0.0001)) // Floor at minimal impact
    }
}

impl ImpactModel for LinearImpact {
    fn estimate(&self, order_size: Quantity, _side: Side, _market: &MarketState) -> Impact {
        let size = order_size.to_f64();
        let impact_bps = self.lambda * size;
        Impact::permanent(impact_bps)
    }

    fn name(&self) -> &str {
        "linear"
    }
}

impl Default for LinearImpact {
    fn default() -> Self {
        // Default lambda of 0.01 bps per unit
        Self::new(0.01)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Price;

    #[test]
    fn test_linear_impact() {
        let model = LinearImpact::new(0.1); // 0.1 bps per unit
        let market = MarketState::default();

        let impact = model.estimate(Quantity::from_int(100), Side::Buy, &market);
        // 100 units × 0.1 bps/unit = 10 bps
        assert!((impact.price_bps - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_from_market() {
        let market = MarketState {
            mid_price: Price::from_int(100),
            spread: Price::from_raw(10_000_000), // 0.1 = 10 bps
            bid_depth: Quantity::from_int(500),
            ask_depth: Quantity::from_int(500),
            ..Default::default()
        };

        let model = LinearImpact::from_market(&market);
        // λ ≈ 10 bps / (2 × 1000) = 0.005 bps per unit
        assert!(model.lambda > 0.0);
        assert!(model.lambda < 0.1);
    }

    #[test]
    fn test_side_symmetric() {
        let model = LinearImpact::new(0.1);
        let market = MarketState::default();

        let buy_impact = model.estimate(Quantity::from_int(100), Side::Buy, &market);
        let sell_impact = model.estimate(Quantity::from_int(100), Side::Sell, &market);

        // Linear model is symmetric (doesn't account for side)
        assert!((buy_impact.price_bps - sell_impact.price_bps).abs() < 0.001);
    }
}
