//! Full L2 Structure Impact Model
//!
//! Multi-dimensional impact model capturing effects on:
//! - Price movement
//! - Spread widening
//! - Depth consumption
//! - Volatility increase
//! - Regime shift probability
//!
//! From docs Section 3: Full L2 Structure Impact
//!
//! This is the most comprehensive impact model, combining elements of
//! square-root and Obizhaeva-Wang with L2 orderbook structure effects.

use super::protocol::ImpactModel;
use crate::domain::{FullImpact, Impact, MarketState};
use trading_core::{Quantity, Side};

/// Full L2 structure impact model configuration
#[derive(Debug, Clone, Copy)]
pub struct FullImpactConfig {
    /// Price impact coefficient (square-root model base)
    pub price_coef: f64,
    /// Spread widening coefficient (α in docs)
    pub spread_alpha: f64,
    /// Cascade multiplier for depth consumption (typically 1.2-1.5)
    pub depth_cascade: f64,
    /// Volatility impact coefficient (β in docs)
    pub volatility_beta: f64,
    /// Base recovery half-life in seconds
    pub base_recovery_secs: f64,
    /// Regime shift threshold (fraction of depth)
    pub regime_threshold: f64,
    /// Regime shift steepness (sigmoid parameter)
    pub regime_steepness: f64,
}

impl Default for FullImpactConfig {
    fn default() -> Self {
        Self {
            price_coef: 0.314,        // Almgren coefficient
            spread_alpha: 0.4,        // Typical 0.3-0.5
            depth_cascade: 1.3,       // Typical 1.2-1.5
            volatility_beta: 0.3,     // Typical 0.2-0.4
            base_recovery_secs: 30.0, // Typical 10-60 seconds
            regime_threshold: 0.2,    // 20% of touch depth
            regime_steepness: 10.0,
        }
    }
}

/// Full L2 structure impact model
pub struct FullImpactModel {
    config: FullImpactConfig,
}

impl FullImpactModel {
    pub fn new(config: FullImpactConfig) -> Self {
        Self { config }
    }

    /// Create with default academic parameters
    pub fn default_config() -> Self {
        Self::new(FullImpactConfig::default())
    }

    /// Create for aggressive/stressed market conditions
    pub fn stressed() -> Self {
        Self::new(FullImpactConfig {
            price_coef: 0.5,
            spread_alpha: 0.6,
            depth_cascade: 1.5,
            volatility_beta: 0.5,
            base_recovery_secs: 60.0,
            regime_threshold: 0.15,
            regime_steepness: 15.0,
        })
    }

    /// Create for calm/liquid market conditions
    pub fn liquid() -> Self {
        Self::new(FullImpactConfig {
            price_coef: 0.2,
            spread_alpha: 0.25,
            depth_cascade: 1.15,
            volatility_beta: 0.15,
            base_recovery_secs: 15.0,
            regime_threshold: 0.3,
            regime_steepness: 5.0,
        })
    }

    /// Calculate participation rate considering side
    fn participation(&self, size: Quantity, side: Side, market: &MarketState) -> f64 {
        let order_size = size.to_f64();
        let side_depth = match side {
            Side::Buy => market.ask_depth.to_f64(),
            Side::Sell => market.bid_depth.to_f64(),
        };
        if side_depth <= 0.0 {
            return 1.0;
        }
        (order_size / side_depth).min(10.0) // Cap at 10x depth
    }

    /// Sigmoid function for smooth transitions
    fn sigmoid(x: f64) -> f64 {
        1.0 / (1.0 + (-x).exp())
    }
}

impl ImpactModel for FullImpactModel {
    fn estimate(&self, order_size: Quantity, side: Side, market: &MarketState) -> Impact {
        let full = self.estimate_full(order_size, side, market);
        Impact::permanent(full.price_impact_bps)
    }

    fn estimate_full(&self, order_size: Quantity, side: Side, market: &MarketState) -> FullImpact {
        let participation = self.participation(order_size, side, market);
        let volatility = market.volatility.max(0.01);
        let config = &self.config;

        // 1. Price Impact (square-root model)
        let price_impact_bps = config.price_coef * volatility * participation.sqrt() * 10_000.0;

        // 2. Spread Impact
        // Spread_after = Spread_before × (1 + α × depth_consumed)
        let spread_impact_pct = config.spread_alpha * participation.min(1.0);

        // 3. Depth Impact (with cascade)
        // Actual consumption = direct + cascade effects
        let direct_depth = participation.min(1.0);
        let depth_impact_pct = (direct_depth * config.depth_cascade).min(1.0);

        // 4. Volatility Impact
        // Vol_after = Vol_before × (1 + β × |imbalance|)
        let imbalance = participation.sqrt().min(1.0);
        let volatility_impact_pct = config.volatility_beta * imbalance;

        // 5. Recovery Half-Life
        // Larger orders take longer to recover
        let recovery_multiplier = 1.0 + participation.sqrt();
        let recovery_half_life_secs = config.base_recovery_secs * recovery_multiplier;

        // 6. Regime Shift Probability
        // P(regime_shift) = σ(k × (depth_ratio - threshold))
        let regime_shift_prob =
            Self::sigmoid(config.regime_steepness * (participation - config.regime_threshold));

        FullImpact {
            price_impact_bps,
            spread_impact_pct,
            depth_impact_pct,
            volatility_impact_pct,
            recovery_half_life_secs,
            regime_shift_prob,
        }
    }

    fn name(&self) -> &str {
        "full_l2"
    }
}

impl Default for FullImpactModel {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Price;

    fn market_with_sides(bid_depth: i64, ask_depth: i64, volatility: f64) -> MarketState {
        MarketState {
            mid_price: Price::from_int(100),
            bid_depth: Quantity::from_int(bid_depth),
            ask_depth: Quantity::from_int(ask_depth),
            volatility,
            ..Default::default()
        }
    }

    #[test]
    fn test_full_impact_dimensions() {
        let model = FullImpactModel::default();
        let market = market_with_sides(1000, 1000, 0.2);

        let impact = model.estimate_full(
            Quantity::from_int(200), // 20% participation
            Side::Buy,
            &market,
        );

        // All dimensions should be positive
        assert!(impact.price_impact_bps > 0.0);
        assert!(impact.spread_impact_pct > 0.0);
        assert!(impact.depth_impact_pct > 0.0);
        assert!(impact.volatility_impact_pct > 0.0);
        assert!(impact.recovery_half_life_secs > 0.0);

        // At 20% participation, regime shift prob should be near threshold
        assert!(impact.regime_shift_prob > 0.4);
        assert!(impact.regime_shift_prob < 0.6);
    }

    #[test]
    fn test_side_aware_participation() {
        let model = FullImpactModel::default();
        // Imbalanced market: more asks than bids
        let market = market_with_sides(500, 2000, 0.2);

        let buy_impact = model.estimate_full(Quantity::from_int(200), Side::Buy, &market);
        let sell_impact = model.estimate_full(Quantity::from_int(200), Side::Sell, &market);

        // Buy should have less impact (more ask depth available)
        assert!(buy_impact.price_impact_bps < sell_impact.price_impact_bps);
    }

    #[test]
    fn test_regime_shift_threshold() {
        let model = FullImpactModel::default();
        let market = market_with_sides(1000, 1000, 0.2);

        // Small order (5%) - participation is 50/1000 on ask side = 5%
        // sigmoid(10 * (0.05 - 0.2)) = sigmoid(-1.5) ≈ 0.18
        let small = model.estimate_full(Quantity::from_int(50), Side::Buy, &market);
        assert!(small.regime_shift_prob < 0.25);

        // Medium order (20% - at threshold)
        // sigmoid(10 * (0.2 - 0.2)) = sigmoid(0) = 0.5
        let medium = model.estimate_full(Quantity::from_int(200), Side::Buy, &market);
        assert!(medium.regime_shift_prob > 0.4);
        assert!(medium.regime_shift_prob < 0.6);

        // Large order (50%)
        // sigmoid(10 * (0.5 - 0.2)) = sigmoid(3) ≈ 0.95
        let large = model.estimate_full(Quantity::from_int(500), Side::Buy, &market);
        assert!(large.regime_shift_prob > 0.9);
    }

    #[test]
    fn test_stressed_vs_liquid() {
        let stressed = FullImpactModel::stressed();
        let liquid = FullImpactModel::liquid();
        let market = market_with_sides(1000, 1000, 0.2);

        let stressed_impact = stressed.estimate_full(Quantity::from_int(100), Side::Buy, &market);
        let liquid_impact = liquid.estimate_full(Quantity::from_int(100), Side::Buy, &market);

        // Stressed market should have higher impact across all dimensions
        assert!(stressed_impact.price_impact_bps > liquid_impact.price_impact_bps);
        assert!(stressed_impact.spread_impact_pct > liquid_impact.spread_impact_pct);
        assert!(stressed_impact.recovery_half_life_secs > liquid_impact.recovery_half_life_secs);
    }

    #[test]
    fn test_cascade_effect() {
        let model = FullImpactModel::default();
        let market = market_with_sides(1000, 1000, 0.2);

        let impact = model.estimate_full(Quantity::from_int(500), Side::Buy, &market);

        // Depth impact should be > participation due to cascade
        let participation = 0.5; // 500/1000
        assert!(impact.depth_impact_pct > participation);
    }
}
