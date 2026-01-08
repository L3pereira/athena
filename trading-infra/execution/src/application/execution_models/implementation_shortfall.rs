//! Implementation Shortfall (Almgren-Chriss) Execution Model
//!
//! Strategy: Minimize expected cost + risk.
//!
//! Minimize: E[Cost] + λ × Var[Cost]
//!
//! Where:
//! - E[Cost] = impact cost + timing risk
//! - Var[Cost] = uncertainty in final cost
//! - λ = risk aversion
//!
//! Optimal solution (Almgren-Chriss 2000):
//! q(t) = Q × sinh(κ(T-t)) / sinh(κT)
//!
//! Where κ = √(λσ²/η)
//! - λ = risk aversion
//! - σ = volatility
//! - η = impact coefficient
//!
//! Shape depends on risk aversion:
//!
//! High λ (risk averse):     Low λ (risk neutral):
//!   ↑                         ↑
//!   │█                        │    ██
//!   │██                       │   ████
//!   │███                      │  ██████
//!   │████                     │ ████████
//!   └────→ time               └────────→ time
//!   (front-loaded)            (even/back-loaded)
//!
//! Best for: Risk-controlled execution with urgency

use super::protocol::ExecutionModel;
use crate::domain::{Adjustment, ExecutionSchedule, MarketConditions, MarketState, Slice};
use chrono::Duration;
use trading_core::Quantity;

/// Implementation Shortfall model configuration
#[derive(Debug, Clone, Copy)]
pub struct ISConfig {
    /// Risk aversion parameter (higher = more front-loaded)
    /// Typical range: 0.0001 to 10
    pub risk_aversion: f64,
    /// Impact coefficient η (higher = more spread out)
    pub impact_coefficient: f64,
    /// Number of slices
    pub num_slices: usize,
    /// Base urgency adjustment
    pub base_urgency: f64,
}

impl Default for ISConfig {
    fn default() -> Self {
        Self {
            risk_aversion: 1.0,      // Moderate risk aversion
            impact_coefficient: 0.1, // Moderate impact
            num_slices: 10,
            base_urgency: 0.6,
        }
    }
}

/// Implementation Shortfall / Almgren-Chriss execution model
pub struct ImplementationShortfallModel {
    config: ISConfig,
}

impl ImplementationShortfallModel {
    pub fn new(config: ISConfig) -> Self {
        Self { config }
    }

    /// Create for risk-averse execution (front-loaded)
    pub fn risk_averse() -> Self {
        Self::new(ISConfig {
            risk_aversion: 5.0,
            ..Default::default()
        })
    }

    /// Create for risk-neutral execution (more even)
    pub fn risk_neutral() -> Self {
        Self::new(ISConfig {
            risk_aversion: 0.1,
            ..Default::default()
        })
    }

    /// Create with custom risk aversion
    pub fn with_risk_aversion(lambda: f64) -> Self {
        Self::new(ISConfig {
            risk_aversion: lambda.max(0.0001),
            ..Default::default()
        })
    }

    /// Calculate κ (kappa) parameter
    fn kappa(&self, volatility: f64) -> f64 {
        let lambda = self.config.risk_aversion;
        let eta = self.config.impact_coefficient;
        let sigma_sq = volatility * volatility;

        // κ = √(λσ²/η)
        ((lambda * sigma_sq) / eta.max(0.0001)).sqrt()
    }

    /// Calculate optimal trading rate at normalized time τ ∈ [0, 1]
    /// Returns fraction of total quantity to trade at this point
    fn optimal_trajectory(&self, tau: f64, kappa: f64) -> f64 {
        // q(τ) = sinh(κ(1-τ)) / sinh(κ)
        // where τ = t/T (normalized time)
        if kappa.abs() < 0.001 {
            // Low kappa → even distribution
            return 1.0;
        }

        let remaining = 1.0 - tau;
        (kappa * remaining).sinh() / kappa.sinh()
    }
}

impl ExecutionModel for ImplementationShortfallModel {
    fn compute_schedule(
        &self,
        target_qty: Quantity,
        horizon: Duration,
        market_state: &MarketState,
    ) -> ExecutionSchedule {
        let num_slices = self.config.num_slices.max(1);
        let volatility = market_state.volatility.max(0.01);
        let kappa = self.kappa(volatility);

        // Calculate cumulative execution fractions
        let mut fractions = Vec::with_capacity(num_slices);

        for i in 0..num_slices {
            let tau = i as f64 / num_slices as f64;
            let tau_next = (i + 1) as f64 / num_slices as f64;

            // Cumulative at this point and next
            let cumulative = 1.0 - self.optimal_trajectory(tau, kappa);
            let cumulative_next = 1.0 - self.optimal_trajectory(tau_next, kappa);

            // Fraction for this slice
            let fraction = cumulative_next - cumulative;
            fractions.push(fraction);
        }

        // Normalize fractions (handle numerical errors)
        let total_frac: f64 = fractions.iter().sum();
        for f in &mut fractions {
            *f /= total_frac;
        }

        // Create slices
        let slice_interval = Duration::seconds((horizon.num_seconds() / num_slices as i64).max(1));

        let mut slices = Vec::with_capacity(num_slices);
        let mut remaining = target_qty.raw();

        for (i, &fraction) in fractions.iter().enumerate() {
            let time_offset = slice_interval * i as i32;

            // Last slice gets remaining
            let slice_qty = if i == num_slices - 1 {
                Quantity::from_raw(remaining)
            } else {
                let qty_raw = (target_qty.raw() as f64 * fraction).round() as i64;
                remaining -= qty_raw;
                Quantity::from_raw(qty_raw)
            };

            // Higher urgency for front-loaded slices
            let urgency = (self.config.base_urgency + fraction).clamp(0.0, 1.0);

            slices.push(Slice::new(time_offset, slice_qty, urgency));
        }

        ExecutionSchedule::new(target_qty, horizon, slices, "Implementation Shortfall")
    }

    fn adjust(
        &self,
        actual_filled: Quantity,
        expected_filled: Quantity,
        conditions: &MarketConditions,
    ) -> Adjustment {
        // IS model is more aggressive about catching up
        let shortfall = expected_filled.to_f64() - actual_filled.to_f64();
        let shortfall_pct = if expected_filled.is_zero() {
            0.0
        } else {
            shortfall / expected_filled.to_f64()
        };

        // Risk-averse: catch up quickly when behind
        if shortfall_pct > 0.1 {
            Adjustment::MoreAggressive
        } else if shortfall_pct < -0.3 {
            // Significantly ahead: slow down
            Adjustment::LessAggressive
        } else if conditions.volatility_ratio > 2.0 {
            // High volatility: be more cautious
            Adjustment::LessAggressive
        } else if conditions.is_adverse() && shortfall_pct > 0.0 {
            // Adverse but behind: still push
            Adjustment::Maintain
        } else {
            Adjustment::Maintain
        }
    }

    fn name(&self) -> &str {
        "implementation_shortfall"
    }
}

impl Default for ImplementationShortfallModel {
    fn default() -> Self {
        Self::new(ISConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Price;

    fn market_with_vol(volatility: f64) -> MarketState {
        MarketState {
            mid_price: Price::from_int(100),
            volatility,
            ..Default::default()
        }
    }

    #[test]
    fn test_total_quantity_preserved() {
        let model = ImplementationShortfallModel::default();
        let schedule = model.compute_schedule(
            Quantity::from_int(1000),
            Duration::hours(1),
            &market_with_vol(0.2),
        );

        let total: i64 = schedule.slices.iter().map(|s| s.quantity.raw()).sum();
        assert_eq!(total, 1000_00000000);
    }

    #[test]
    fn test_front_loading_with_risk_aversion() {
        let risk_averse = ImplementationShortfallModel::risk_averse();
        let risk_neutral = ImplementationShortfallModel::risk_neutral();

        let averse_schedule = risk_averse.compute_schedule(
            Quantity::from_int(1000),
            Duration::hours(1),
            &market_with_vol(0.3),
        );

        let neutral_schedule = risk_neutral.compute_schedule(
            Quantity::from_int(1000),
            Duration::hours(1),
            &market_with_vol(0.3),
        );

        // Risk-averse should have more in first half
        let averse_first_half: i64 = averse_schedule.slices[..5]
            .iter()
            .map(|s| s.quantity.raw())
            .sum();
        let neutral_first_half: i64 = neutral_schedule.slices[..5]
            .iter()
            .map(|s| s.quantity.raw())
            .sum();

        assert!(averse_first_half > neutral_first_half);
    }

    #[test]
    fn test_volatility_sensitivity() {
        let model = ImplementationShortfallModel::with_risk_aversion(2.0);

        let low_vol = model.compute_schedule(
            Quantity::from_int(1000),
            Duration::hours(1),
            &market_with_vol(0.1),
        );

        let high_vol = model.compute_schedule(
            Quantity::from_int(1000),
            Duration::hours(1),
            &market_with_vol(0.5),
        );

        // Higher volatility should lead to more front-loading
        let low_vol_first: i64 = low_vol.slices[..3].iter().map(|s| s.quantity.raw()).sum();
        let high_vol_first: i64 = high_vol.slices[..3].iter().map(|s| s.quantity.raw()).sum();

        assert!(high_vol_first > low_vol_first);
    }

    #[test]
    fn test_kappa_calculation() {
        let model = ImplementationShortfallModel::new(ISConfig {
            risk_aversion: 4.0,
            impact_coefficient: 0.1,
            ..Default::default()
        });

        let kappa = model.kappa(0.2);
        // κ = √(4 × 0.04 / 0.1) = √1.6 ≈ 1.26
        assert!((kappa - 1.26).abs() < 0.1);
    }

    #[test]
    fn test_aggressive_adjustment_when_behind() {
        let model = ImplementationShortfallModel::risk_averse();
        let conditions = MarketConditions::default();

        let adj = model.adjust(
            Quantity::from_int(80),  // actual
            Quantity::from_int(100), // expected (20% behind)
            &conditions,
        );

        assert_eq!(adj, Adjustment::MoreAggressive);
    }
}
