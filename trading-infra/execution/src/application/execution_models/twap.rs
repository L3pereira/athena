//! TWAP (Time-Weighted Average Price) Execution Model
//!
//! Strategy: Trade evenly over time.
//!
//! q(t) = Q / T  (constant rate)
//!
//! Timeline:
//! ├──────┼──────┼──────┼──────┤
//!    25%    25%    25%    25%
//!
//! Pros:
//! - Simple, robust
//! - Low information leakage
//! - Benchmark for comparison
//!
//! Cons:
//! - Ignores volume patterns
//! - Doesn't adapt to conditions
//!
//! Best for: Low urgency, want simplicity

use super::protocol::ExecutionModel;
use crate::domain::{ExecutionSchedule, MarketState, Slice};
use chrono::Duration;
use trading_core::Quantity;

/// TWAP execution model configuration
#[derive(Debug, Clone, Copy)]
pub struct TwapConfig {
    /// Number of slices to divide execution into
    pub num_slices: usize,
    /// Base urgency level (0.0 = passive, 1.0 = aggressive)
    pub base_urgency: f64,
    /// Minimum slice interval (prevents too frequent trading)
    pub min_slice_interval_secs: i64,
}

impl Default for TwapConfig {
    fn default() -> Self {
        Self {
            num_slices: 10,
            base_urgency: 0.5,
            min_slice_interval_secs: 60,
        }
    }
}

/// TWAP execution model
pub struct TwapModel {
    config: TwapConfig,
}

impl TwapModel {
    pub fn new(config: TwapConfig) -> Self {
        Self { config }
    }

    /// Create with specified number of slices
    pub fn with_slices(num_slices: usize) -> Self {
        Self::new(TwapConfig {
            num_slices: num_slices.max(1),
            ..Default::default()
        })
    }

    /// Create with specified urgency
    pub fn with_urgency(urgency: f64) -> Self {
        Self::new(TwapConfig {
            base_urgency: urgency.clamp(0.0, 1.0),
            ..Default::default()
        })
    }
}

impl ExecutionModel for TwapModel {
    fn compute_schedule(
        &self,
        target_qty: Quantity,
        horizon: Duration,
        _market_state: &MarketState,
    ) -> ExecutionSchedule {
        let num_slices = self.config.num_slices.max(1);

        // Calculate slice interval
        let total_secs = horizon.num_seconds().max(1);
        let slice_interval = Duration::seconds(
            (total_secs / num_slices as i64).max(self.config.min_slice_interval_secs),
        );

        // Calculate actual number of slices that fit
        let actual_slices = (total_secs / slice_interval.num_seconds()).max(1) as usize;

        // Calculate quantity per slice (even distribution)
        let qty_per_slice_raw = target_qty.raw() / actual_slices as i64;
        let remainder = target_qty.raw() % actual_slices as i64;

        let mut slices = Vec::with_capacity(actual_slices);
        for i in 0..actual_slices {
            let time_offset = slice_interval * i as i32;

            // Add remainder to first slice
            let slice_qty = if i == 0 {
                Quantity::from_raw(qty_per_slice_raw + remainder)
            } else {
                Quantity::from_raw(qty_per_slice_raw)
            };

            slices.push(Slice::new(time_offset, slice_qty, self.config.base_urgency));
        }

        ExecutionSchedule::new(target_qty, horizon, slices, "TWAP")
    }

    fn name(&self) -> &str {
        "twap"
    }
}

impl Default for TwapModel {
    fn default() -> Self {
        Self::new(TwapConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Price;

    fn default_market() -> MarketState {
        MarketState {
            mid_price: Price::from_int(100),
            ..Default::default()
        }
    }

    #[test]
    fn test_even_distribution() {
        let model = TwapModel::with_slices(4);
        let schedule = model.compute_schedule(
            Quantity::from_int(1000),
            Duration::minutes(10),
            &default_market(),
        );

        assert_eq!(schedule.len(), 4);

        // Each slice should be ~250
        let total: i64 = schedule.slices.iter().map(|s| s.quantity.raw()).sum();
        assert_eq!(total, 1000_00000000);
    }

    #[test]
    fn test_time_spacing() {
        let model = TwapModel::with_slices(5);
        let schedule = model.compute_schedule(
            Quantity::from_int(100),
            Duration::minutes(10),
            &default_market(),
        );

        // Verify slices are evenly spaced
        for (i, slice) in schedule.slices.iter().enumerate() {
            let expected_offset = Duration::minutes(2) * i as i32;
            assert_eq!(slice.time_offset, expected_offset);
        }
    }

    #[test]
    fn test_single_slice() {
        let model = TwapModel::with_slices(1);
        let schedule = model.compute_schedule(
            Quantity::from_int(100),
            Duration::minutes(5),
            &default_market(),
        );

        assert_eq!(schedule.len(), 1);
        assert_eq!(schedule.slices[0].quantity.raw(), 100_00000000);
    }

    #[test]
    fn test_urgency_applied() {
        let model = TwapModel::with_urgency(0.8);
        let schedule = model.compute_schedule(
            Quantity::from_int(100),
            Duration::minutes(10),
            &default_market(),
        );

        for slice in &schedule.slices {
            assert!((slice.urgency - 0.8).abs() < 0.001);
        }
    }

    #[test]
    fn test_remainder_handling() {
        let model = TwapModel::with_slices(3);
        let schedule = model.compute_schedule(
            Quantity::from_raw(100), // 100 raw units, not evenly divisible by 3
            Duration::minutes(9),
            &default_market(),
        );

        let total: i64 = schedule.slices.iter().map(|s| s.quantity.raw()).sum();
        assert_eq!(total, 100);

        // First slice gets the remainder
        assert!(schedule.slices[0].quantity.raw() >= schedule.slices[1].quantity.raw());
    }
}
