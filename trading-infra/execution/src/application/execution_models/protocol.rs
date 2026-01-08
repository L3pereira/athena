//! Execution Model Protocol
//!
//! Core trait for execution scheduling (SOLID: OCP)

use crate::domain::{Adjustment, ExecutionSchedule, MarketConditions, MarketState};
use chrono::Duration;
use trading_core::Quantity;

/// Execution model interface (Open for extension, closed for modification)
///
/// Implementations compute optimal execution schedules and adjust based on conditions.
/// All implementations must be thread-safe (Send + Sync).
pub trait ExecutionModel: Send + Sync {
    /// Compute an execution schedule for a target quantity
    ///
    /// # Arguments
    /// * `target_qty` - Total quantity to execute
    /// * `horizon` - Time horizon for execution
    /// * `market_state` - Current market conditions
    ///
    /// # Returns
    /// An execution schedule with slices
    fn compute_schedule(
        &self,
        target_qty: Quantity,
        horizon: Duration,
        market_state: &MarketState,
    ) -> ExecutionSchedule;

    /// Adjust the schedule based on actual progress vs expected
    ///
    /// # Arguments
    /// * `actual_filled` - Quantity actually filled so far
    /// * `expected_filled` - Quantity expected to be filled by now
    /// * `conditions` - Current market conditions
    ///
    /// # Returns
    /// Adjustment signal for next execution slice
    fn adjust(
        &self,
        actual_filled: Quantity,
        expected_filled: Quantity,
        conditions: &MarketConditions,
    ) -> Adjustment {
        let shortfall = expected_filled.to_f64() - actual_filled.to_f64();
        let shortfall_pct = if expected_filled.is_zero() {
            0.0
        } else {
            shortfall / expected_filled.to_f64()
        };

        if shortfall_pct > 0.2 {
            // More than 20% behind
            Adjustment::MoreAggressive
        } else if shortfall_pct < -0.2 {
            // More than 20% ahead
            Adjustment::LessAggressive
        } else if conditions.is_adverse() {
            Adjustment::LessAggressive
        } else if conditions.is_favorable() && shortfall_pct > 0.0 {
            Adjustment::MoreAggressive
        } else {
            Adjustment::Maintain
        }
    }

    /// Get the model name for logging/debugging
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Slice;
    use trading_core::Price;

    struct DummyModel;

    impl ExecutionModel for DummyModel {
        fn compute_schedule(
            &self,
            target_qty: Quantity,
            horizon: Duration,
            _market: &MarketState,
        ) -> ExecutionSchedule {
            ExecutionSchedule::new(
                target_qty,
                horizon,
                vec![Slice::new(Duration::zero(), target_qty, 0.5)],
                "dummy",
            )
        }

        fn name(&self) -> &str {
            "dummy"
        }
    }

    #[test]
    fn test_default_adjust_behind() {
        let model = DummyModel;
        let conditions = MarketConditions::default();

        let adj = model.adjust(
            Quantity::from_int(70),  // actual
            Quantity::from_int(100), // expected
            &conditions,
        );

        assert_eq!(adj, Adjustment::MoreAggressive);
    }

    #[test]
    fn test_default_adjust_ahead() {
        let model = DummyModel;
        let conditions = MarketConditions::default();

        let adj = model.adjust(
            Quantity::from_int(130), // actual
            Quantity::from_int(100), // expected
            &conditions,
        );

        assert_eq!(adj, Adjustment::LessAggressive);
    }

    #[test]
    fn test_default_adjust_on_track() {
        let model = DummyModel;
        // Use neutral conditions (not favorable) to test Maintain path
        let conditions = MarketConditions {
            spread_ratio: 1.5,     // Slightly wide
            depth_ratio: 0.8,      // Slightly low
            volatility_ratio: 1.5, // Slightly high
            fill_rate: 0.4,        // Below 50%
            ..Default::default()
        };

        let adj = model.adjust(
            Quantity::from_int(95),  // actual (within 10%)
            Quantity::from_int(100), // expected
            &conditions,
        );

        assert_eq!(adj, Adjustment::Maintain);
    }
}
