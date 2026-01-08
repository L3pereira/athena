//! Adaptive Execution Model
//!
//! Strategy: Adjust based on real-time conditions.
//!
//! At each step:
//! 1. Observe: spread, depth, volatility, fill rate
//! 2. Compare: actual vs expected progress
//! 3. Adjust: speed up if behind, slow down if ahead
//!
//! Triggers for adjustment:
//! - Spread widens → slow down (market stressed)
//! - Depth increases → speed up (opportunity)
//! - Volatility spikes → reduce size (uncertainty)
//! - Fill rate low → become more aggressive
//!
//! Best for: Variable market conditions

use super::protocol::ExecutionModel;
use crate::domain::{Adjustment, ExecutionSchedule, MarketConditions, MarketState, Slice};
use chrono::Duration;
use trading_core::Quantity;

/// Adaptive model configuration
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveConfig {
    /// Base number of slices (will adapt)
    pub base_slices: usize,
    /// Shortfall threshold for becoming aggressive (%)
    pub aggressive_threshold: f64,
    /// Ahead threshold for becoming passive (%)
    pub passive_threshold: f64,
    /// Spread widening threshold (multiple of normal)
    pub spread_warning: f64,
    /// Volatility spike threshold (multiple of normal)
    pub volatility_warning: f64,
    /// Minimum fill rate threshold
    pub min_fill_rate: f64,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            base_slices: 10,
            aggressive_threshold: 0.15, // 15% behind → aggressive
            passive_threshold: 0.20,    // 20% ahead → passive
            spread_warning: 2.0,        // 2x normal spread
            volatility_warning: 2.5,    // 2.5x normal volatility
            min_fill_rate: 0.3,         // Below 30% fill rate → adjust
        }
    }
}

/// Adaptive execution model with real-time adjustments
pub struct AdaptiveModel {
    config: AdaptiveConfig,
}

impl AdaptiveModel {
    pub fn new(config: AdaptiveConfig) -> Self {
        Self { config }
    }

    /// Create with aggressive settings (faster reaction)
    pub fn aggressive() -> Self {
        Self::new(AdaptiveConfig {
            aggressive_threshold: 0.1,
            passive_threshold: 0.3,
            spread_warning: 1.5,
            ..Default::default()
        })
    }

    /// Create with conservative settings (slower reaction)
    pub fn conservative() -> Self {
        Self::new(AdaptiveConfig {
            aggressive_threshold: 0.25,
            passive_threshold: 0.15,
            spread_warning: 3.0,
            volatility_warning: 3.5,
            ..Default::default()
        })
    }

    /// Compute adaptive urgency based on market conditions
    #[allow(dead_code)]
    pub fn compute_urgency(&self, conditions: &MarketConditions, base_urgency: f64) -> f64 {
        let mut urgency = base_urgency;

        // Spread widening → lower urgency
        if conditions.spread_ratio > self.config.spread_warning {
            urgency *= 0.5;
        } else if conditions.spread_ratio > 1.5 {
            urgency *= 0.8;
        }

        // High volatility → lower urgency
        if conditions.volatility_ratio > self.config.volatility_warning {
            urgency *= 0.4;
        } else if conditions.volatility_ratio > 1.5 {
            urgency *= 0.7;
        }

        // Good depth → higher urgency
        if conditions.depth_ratio > 1.5 {
            urgency *= 1.3;
        }

        // Low fill rate → higher urgency (need to be more aggressive)
        if conditions.fill_rate < self.config.min_fill_rate {
            urgency *= 1.4;
        }

        urgency.clamp(0.1, 1.0)
    }
}

impl ExecutionModel for AdaptiveModel {
    fn compute_schedule(
        &self,
        target_qty: Quantity,
        horizon: Duration,
        market_state: &MarketState,
    ) -> ExecutionSchedule {
        // Start with TWAP-like base schedule
        // Actual adaptation happens via adjust() method
        let num_slices = self.config.base_slices.max(1);
        let slice_interval = Duration::seconds((horizon.num_seconds() / num_slices as i64).max(60));

        let qty_per_slice = target_qty.raw() / num_slices as i64;
        let remainder = target_qty.raw() % num_slices as i64;

        // Base urgency depends on market conditions
        let base_urgency = if market_state.spread_bps() > 50.0 {
            0.4 // Wide spread → cautious
        } else if market_state.volatility > 0.3 {
            0.5 // High vol → moderate
        } else {
            0.6 // Normal → standard
        };

        let mut slices = Vec::with_capacity(num_slices);
        for i in 0..num_slices {
            let time_offset = slice_interval * i as i32;
            let slice_qty = if i == 0 {
                Quantity::from_raw(qty_per_slice + remainder)
            } else {
                Quantity::from_raw(qty_per_slice)
            };

            slices.push(Slice::new(time_offset, slice_qty, base_urgency));
        }

        ExecutionSchedule::new(target_qty, horizon, slices, "Adaptive")
    }

    fn adjust(
        &self,
        actual_filled: Quantity,
        expected_filled: Quantity,
        conditions: &MarketConditions,
    ) -> Adjustment {
        let shortfall_pct = if expected_filled.is_zero() {
            0.0
        } else {
            (expected_filled.to_f64() - actual_filled.to_f64()) / expected_filled.to_f64()
        };

        // Check for extreme conditions first
        if conditions.spread_ratio > self.config.spread_warning * 1.5 {
            return Adjustment::Pause; // Very wide spread → pause
        }

        if conditions.volatility_ratio > self.config.volatility_warning * 1.5 {
            return Adjustment::Pause; // Extreme volatility → pause
        }

        // Significantly behind schedule
        if shortfall_pct > self.config.aggressive_threshold {
            if conditions.is_favorable() {
                return Adjustment::MoreAggressive;
            } else if conditions.is_adverse() {
                // Behind but adverse conditions: maintain (don't chase)
                return Adjustment::Maintain;
            }
            return Adjustment::MoreAggressive;
        }

        // Significantly ahead of schedule
        if shortfall_pct < -self.config.passive_threshold {
            return Adjustment::LessAggressive;
        }

        // Spread warning
        if conditions.spread_ratio > self.config.spread_warning {
            return Adjustment::LessAggressive;
        }

        // Volatility warning
        if conditions.volatility_ratio > self.config.volatility_warning {
            return Adjustment::LessAggressive;
        }

        // Low fill rate but not ahead
        if conditions.fill_rate < self.config.min_fill_rate && shortfall_pct >= 0.0 {
            return Adjustment::MoreAggressive;
        }

        // Good conditions and on track
        if conditions.is_favorable() && shortfall_pct.abs() < 0.05 {
            // Slightly more aggressive to take advantage
            return Adjustment::MoreAggressive;
        }

        Adjustment::Maintain
    }

    fn name(&self) -> &str {
        "adaptive"
    }
}

impl Default for AdaptiveModel {
    fn default() -> Self {
        Self::new(AdaptiveConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Price;

    fn normal_market() -> MarketState {
        MarketState {
            mid_price: Price::from_int(100),
            spread: Price::from_raw(5_000_000), // 5 bps
            volatility: 0.2,
            ..Default::default()
        }
    }

    fn normal_conditions() -> MarketConditions {
        MarketConditions::default()
    }

    #[test]
    fn test_schedule_creation() {
        let model = AdaptiveModel::default();
        let schedule = model.compute_schedule(
            Quantity::from_int(1000),
            Duration::hours(1),
            &normal_market(),
        );

        let total: i64 = schedule.slices.iter().map(|s| s.quantity.raw()).sum();
        assert_eq!(total, 1000_00000000);
        assert_eq!(schedule.model_name, "Adaptive");
    }

    #[test]
    fn test_pause_on_extreme_spread() {
        let model = AdaptiveModel::default();
        let conditions = MarketConditions {
            spread_ratio: 4.0, // 4x normal spread
            ..Default::default()
        };

        let adj = model.adjust(Quantity::from_int(50), Quantity::from_int(50), &conditions);

        assert_eq!(adj, Adjustment::Pause);
    }

    #[test]
    fn test_aggressive_when_behind() {
        let model = AdaptiveModel::default();

        let adj = model.adjust(
            Quantity::from_int(70), // 30% behind
            Quantity::from_int(100),
            &normal_conditions(),
        );

        assert_eq!(adj, Adjustment::MoreAggressive);
    }

    #[test]
    fn test_less_aggressive_when_ahead() {
        let model = AdaptiveModel::default();

        let adj = model.adjust(
            Quantity::from_int(130), // 30% ahead
            Quantity::from_int(100),
            &normal_conditions(),
        );

        assert_eq!(adj, Adjustment::LessAggressive);
    }

    #[test]
    fn test_maintain_on_track() {
        let model = AdaptiveModel::default();
        // Use neutral conditions (not favorable) to test Maintain path
        let neutral_conditions = MarketConditions {
            spread_ratio: 1.5,     // Slightly wide
            depth_ratio: 0.8,      // Slightly low
            volatility_ratio: 1.5, // Slightly high
            fill_rate: 0.4,        // Below 50%
            ..Default::default()
        };

        let adj = model.adjust(
            Quantity::from_int(98), // ~2% behind (within tolerance)
            Quantity::from_int(100),
            &neutral_conditions,
        );

        assert_eq!(adj, Adjustment::Maintain);
    }

    #[test]
    fn test_aggressive_on_favorable() {
        let model = AdaptiveModel::default();
        let favorable = MarketConditions {
            spread_ratio: 0.5,
            depth_ratio: 1.5,
            volatility_ratio: 0.5,
            fill_rate: 0.9,
            ..Default::default()
        };

        let adj = model.adjust(
            Quantity::from_int(99), // On track
            Quantity::from_int(100),
            &favorable,
        );

        assert_eq!(adj, Adjustment::MoreAggressive);
    }

    #[test]
    fn test_less_aggressive_on_volatility() {
        let model = AdaptiveModel::default();
        let volatile = MarketConditions {
            volatility_ratio: 3.0, // 3x normal
            ..Default::default()
        };

        let adj = model.adjust(Quantity::from_int(100), Quantity::from_int(100), &volatile);

        assert_eq!(adj, Adjustment::LessAggressive);
    }

    #[test]
    fn test_low_fill_rate_triggers_aggressive() {
        let model = AdaptiveModel::default();
        let poor_fills = MarketConditions {
            fill_rate: 0.1, // Only 10% filling
            ..Default::default()
        };

        let adj = model.adjust(
            Quantity::from_int(90), // Slightly behind
            Quantity::from_int(100),
            &poor_fills,
        );

        assert_eq!(adj, Adjustment::MoreAggressive);
    }
}
