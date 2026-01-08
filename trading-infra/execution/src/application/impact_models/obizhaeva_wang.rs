//! Obizhaeva-Wang Impact Model
//!
//! Adds TIME DYNAMICS to impact modeling.
//!
//! Price(t) = P₀ + Permanent_Impact + Transient_Impact(t)
//!
//! Transient decays: I_transient(t) = I₀ × e^(-ρt)
//!
//! Where:
//! - ρ = resilience rate (how fast book recovers)
//!
//! Key insight: Impact has two components:
//! 1. Permanent: Information content (doesn't decay)
//! 2. Transient: Mechanical consumption (decays as book refills)
//!
//! Best for: Multi-day execution, understanding recovery dynamics

use super::protocol::ImpactModel;
use crate::domain::{FullImpact, Impact, MarketState};
use trading_core::{Quantity, Side};

/// Obizhaeva-Wang impact model with resilience dynamics
pub struct ObizhaevaWangImpact {
    /// Permanent impact coefficient
    permanent_coef: f64,
    /// Transient impact coefficient (initial)
    transient_coef: f64,
    /// Resilience rate (decay constant, per second)
    resilience_rate: f64,
    /// Time horizon for transient impact calculation (seconds)
    time_horizon_secs: f64,
}

impl ObizhaevaWangImpact {
    /// Create a new Obizhaeva-Wang model
    ///
    /// # Arguments
    /// * `permanent_coef` - Coefficient for permanent impact
    /// * `transient_coef` - Coefficient for initial transient impact
    /// * `resilience_rate` - Decay rate (higher = faster recovery)
    pub fn new(permanent_coef: f64, transient_coef: f64, resilience_rate: f64) -> Self {
        Self {
            permanent_coef,
            transient_coef,
            resilience_rate,
            time_horizon_secs: 60.0, // Default 1 minute horizon
        }
    }

    /// Create with default academic parameters
    /// From Obizhaeva & Wang (2013)
    pub fn academic() -> Self {
        Self::new(
            0.1,  // 10% of impact is permanent
            0.9,  // 90% is transient
            0.05, // ~20 second half-life (ln(2)/0.05 ≈ 14s)
        )
    }

    /// Create for highly resilient markets (HFT)
    pub fn high_resilience() -> Self {
        Self::new(0.05, 0.95, 0.2) // ~3.5 second half-life
    }

    /// Create for less resilient markets
    pub fn low_resilience() -> Self {
        Self::new(0.2, 0.8, 0.01) // ~70 second half-life
    }

    /// Set the time horizon for transient calculations
    pub fn with_horizon(mut self, seconds: f64) -> Self {
        self.time_horizon_secs = seconds.max(1.0);
        self
    }

    /// Calculate transient impact at time t after trade
    pub fn transient_at_time(&self, initial_transient: f64, time_secs: f64) -> f64 {
        initial_transient * (-self.resilience_rate * time_secs).exp()
    }

    /// Calculate recovery half-life in seconds
    pub fn half_life_secs(&self) -> f64 {
        if self.resilience_rate <= 0.0 {
            return f64::INFINITY;
        }
        (2.0_f64).ln() / self.resilience_rate
    }
}

impl ImpactModel for ObizhaevaWangImpact {
    fn estimate(&self, order_size: Quantity, side: Side, market: &MarketState) -> Impact {
        let size = order_size.to_f64();
        let depth = market.total_depth().to_f64().max(1.0);
        let volatility = market.volatility.max(0.01);

        // Base impact using square-root scaling
        let participation = size / depth;
        let base_impact = volatility * participation.sqrt() * 10_000.0;

        // Split into permanent and transient
        let permanent = base_impact * self.permanent_coef;
        let transient = base_impact * self.transient_coef;

        // Average transient over time horizon
        let avg_transient = if self.resilience_rate > 0.0 {
            transient * (1.0 - (-self.resilience_rate * self.time_horizon_secs).exp())
                / (self.resilience_rate * self.time_horizon_secs)
        } else {
            transient
        };

        // Total impact = permanent + time-weighted transient
        let total = permanent + avg_transient;

        // Sign by side
        let signed = match side {
            Side::Buy => total,
            Side::Sell => -total,
        };

        Impact::permanent(signed.abs()) // Report as absolute for comparison
    }

    fn estimate_full(&self, order_size: Quantity, _side: Side, market: &MarketState) -> FullImpact {
        let size = order_size.to_f64();
        let depth = market.total_depth().to_f64().max(1.0);
        let volatility = market.volatility.max(0.01);

        // Base calculations
        let participation = size / depth;
        let base_impact = volatility * participation.sqrt() * 10_000.0;

        let permanent = base_impact * self.permanent_coef;
        let transient = base_impact * self.transient_coef;

        // Depth impact based on participation
        let depth_impact = participation.min(1.0);

        // Spread widening (proportional to depth consumed)
        let spread_impact = 0.3 * depth_impact; // Typical α ≈ 0.3

        // Volatility impact
        let vol_impact = 0.2 * participation.sqrt().min(1.0);

        // Regime shift probability (sigmoid)
        let threshold = 0.2;
        let steepness = 10.0;
        let regime_prob = 1.0 / (1.0 + (steepness * (threshold - participation)).exp());

        FullImpact {
            price_impact_bps: permanent + transient,
            spread_impact_pct: spread_impact,
            depth_impact_pct: depth_impact,
            volatility_impact_pct: vol_impact,
            recovery_half_life_secs: self.half_life_secs(),
            regime_shift_prob: regime_prob,
        }
    }

    fn name(&self) -> &str {
        "obizhaeva_wang"
    }
}

impl Default for ObizhaevaWangImpact {
    fn default() -> Self {
        Self::academic()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Price;

    fn market_with_depth(depth: i64, volatility: f64) -> MarketState {
        MarketState {
            mid_price: Price::from_int(100),
            bid_depth: Quantity::from_int(depth / 2),
            ask_depth: Quantity::from_int(depth / 2),
            volatility,
            ..Default::default()
        }
    }

    #[test]
    fn test_half_life() {
        let model = ObizhaevaWangImpact::new(0.1, 0.9, 0.05);
        let half_life = model.half_life_secs();
        // ln(2) / 0.05 ≈ 13.86
        assert!((half_life - 13.86).abs() < 0.5);
    }

    #[test]
    fn test_transient_decay() {
        let model = ObizhaevaWangImpact::academic();
        let initial = 100.0;

        let at_0 = model.transient_at_time(initial, 0.0);
        let at_half = model.transient_at_time(initial, model.half_life_secs());
        let at_5half = model.transient_at_time(initial, 5.0 * model.half_life_secs());

        assert!((at_0 - 100.0).abs() < 0.01);
        assert!((at_half - 50.0).abs() < 1.0);
        assert!(at_5half < 5.0); // After 5 half-lives, ~3% remaining
    }

    #[test]
    fn test_permanent_vs_transient() {
        let model = ObizhaevaWangImpact::new(0.2, 0.8, 0.05);
        let market = market_with_depth(1000, 0.2);

        let impact = model.estimate(Quantity::from_int(100), Side::Buy, &market);

        // Impact should be positive
        assert!(impact.price_bps > 0.0);
    }

    #[test]
    fn test_full_impact() {
        let model = ObizhaevaWangImpact::academic();
        let market = market_with_depth(1000, 0.2);

        // Large order consuming 30% of depth
        let full = model.estimate_full(Quantity::from_int(300), Side::Buy, &market);

        assert!(full.price_impact_bps > 0.0);
        assert!(full.spread_impact_pct > 0.0);
        assert!(full.depth_impact_pct > 0.2);
        assert!(full.recovery_half_life_secs > 0.0);
        assert!(full.regime_shift_prob > 0.3); // Large order triggers regime shift
    }

    #[test]
    fn test_resilience_comparison() {
        let high_res = ObizhaevaWangImpact::high_resilience();
        let low_res = ObizhaevaWangImpact::low_resilience();

        assert!(high_res.half_life_secs() < low_res.half_life_secs());
    }
}
