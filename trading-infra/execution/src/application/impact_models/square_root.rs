//! Square-Root Impact Model
//!
//! The most empirically validated model.
//!
//! Impact = σ × (Q / V)^0.5
//!
//! Where:
//! - σ = volatility (daily or annualized)
//! - Q = order size
//! - V = average daily volume (or depth)
//!
//! Key insight: Impact is CONCAVE in size
//! - Doubling size does NOT double impact
//! - First 1,000 shares: 10 bps impact
//! - Next 1,000 shares: 7 bps additional (not 10!)
//!
//! From Almgren et al. (2005):
//! Permanent Impact ≈ 0.314 × σ_daily × (Q/V)^0.5
//!
//! Best for: Most situations, especially medium-large orders

use super::protocol::ImpactModel;
use crate::domain::{Impact, MarketState};
use trading_core::{Quantity, Side};

/// Square-root impact model (Almgren et al. 2005)
pub struct SquareRootImpact {
    /// Scaling coefficient (typically ~0.314 for permanent impact)
    coefficient: f64,
    /// Exponent (typically 0.5, but can be calibrated)
    exponent: f64,
}

impl SquareRootImpact {
    /// Create a new square-root impact model
    ///
    /// # Arguments
    /// * `coefficient` - Scaling factor (default 0.314 from Almgren)
    pub fn new(coefficient: f64) -> Self {
        Self {
            coefficient,
            exponent: 0.5,
        }
    }

    /// Create with custom exponent (for calibration)
    pub fn with_exponent(coefficient: f64, exponent: f64) -> Self {
        Self {
            coefficient,
            exponent: exponent.clamp(0.1, 1.0),
        }
    }

    /// Almgren's empirically validated model
    /// Impact = 0.314 × σ × (Q/V)^0.5
    pub fn almgren() -> Self {
        Self::new(0.314)
    }

    /// Conservative estimate (higher impact)
    pub fn conservative() -> Self {
        Self::new(0.5)
    }
}

impl ImpactModel for SquareRootImpact {
    fn estimate(&self, order_size: Quantity, _side: Side, market: &MarketState) -> Impact {
        let size = order_size.to_f64();
        let volume = market.daily_volume.to_f64().max(1.0);
        let volatility = market.volatility.max(0.01); // Floor at 1% vol

        // Impact = coefficient × volatility × (size/volume)^exponent
        let participation = (size / volume).max(0.0);
        let impact_bps =
            self.coefficient * volatility * participation.powf(self.exponent) * 10_000.0;

        Impact::permanent(impact_bps)
    }

    fn name(&self) -> &str {
        "square_root"
    }
}

impl Default for SquareRootImpact {
    fn default() -> Self {
        Self::almgren()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Price;

    fn market_with_volume(daily_vol: i64, volatility: f64) -> MarketState {
        MarketState {
            mid_price: Price::from_int(100),
            daily_volume: Quantity::from_int(daily_vol),
            volatility,
            ..Default::default()
        }
    }

    #[test]
    fn test_square_root_impact() {
        let model = SquareRootImpact::almgren();
        let market = market_with_volume(100_000, 0.2); // 20% vol

        let impact = model.estimate(Quantity::from_int(1000), Side::Buy, &market);
        // 0.314 × 0.2 × (1000/100000)^0.5 × 10000 = 62.8 bps
        assert!(impact.price_bps > 50.0);
        assert!(impact.price_bps < 80.0);
    }

    #[test]
    fn test_concavity() {
        let model = SquareRootImpact::almgren();
        let market = market_with_volume(100_000, 0.2);

        let impact_1k = model.estimate(Quantity::from_int(1000), Side::Buy, &market);
        let impact_2k = model.estimate(Quantity::from_int(2000), Side::Buy, &market);
        let impact_4k = model.estimate(Quantity::from_int(4000), Side::Buy, &market);

        // Impact should be concave: doubling size < doubles impact
        assert!(impact_2k.price_bps < impact_1k.price_bps * 2.0);
        assert!(impact_4k.price_bps < impact_2k.price_bps * 2.0);

        // Specifically: √2 ≈ 1.414, so 2x size → ~1.41x impact
        let ratio = impact_2k.price_bps / impact_1k.price_bps;
        assert!((ratio - 1.414).abs() < 0.1);
    }

    #[test]
    fn test_volatility_scaling() {
        let model = SquareRootImpact::almgren();
        let low_vol = market_with_volume(100_000, 0.1);
        let high_vol = market_with_volume(100_000, 0.3);

        let impact_low = model.estimate(Quantity::from_int(1000), Side::Buy, &low_vol);
        let impact_high = model.estimate(Quantity::from_int(1000), Side::Buy, &high_vol);

        // Higher vol → higher impact (linear relationship)
        assert!(impact_high.price_bps > impact_low.price_bps * 2.5);
    }

    #[test]
    fn test_volume_scaling() {
        let model = SquareRootImpact::almgren();
        let low_volume = market_with_volume(50_000, 0.2);
        let high_volume = market_with_volume(200_000, 0.2);

        let impact_low = model.estimate(Quantity::from_int(1000), Side::Buy, &low_volume);
        let impact_high = model.estimate(Quantity::from_int(1000), Side::Buy, &high_volume);

        // More volume → lower impact (inverse sqrt relationship)
        assert!(impact_high.price_bps < impact_low.price_bps);
    }
}
