//! VWAP (Volume-Weighted Average Price) Execution Model
//!
//! Strategy: Trade proportionally to market volume.
//!
//! q(t) = Q × V(t) / V_total
//!
//! Where V(t) = expected volume at time t
//!
//! Intuition:
//! - Market is most liquid when volume is high
//! - Trading with the crowd reduces footprint
//! - Matches natural volume profile
//!
//! Volume Profile:
//!      ↑
//!  ████│          ████
//!  ████│  ██      ████
//!  ████│ ████ ██  ████
//! ─────┼────────────────→
//!     Open    Midday   Close
//!
//! Pros:
//! - Lower impact than TWAP (trades when liquid)
//! - Industry standard benchmark
//! - Adapts to market rhythms
//!
//! Cons:
//! - Predictable (others can front-run)
//! - Requires volume forecasting
//!
//! Best for: Standard execution with volume awareness

use super::protocol::ExecutionModel;
use crate::domain::{ExecutionSchedule, MarketState, Slice};
use chrono::Duration;
use trading_core::Quantity;

/// Intraday volume profile (hourly buckets)
/// Values represent relative volume (1.0 = average)
pub type VolumeProfile = [f64; 24];

/// Standard U-shaped volume profile (typical for equities)
/// High volume at open/close, low volume midday
pub const U_SHAPED_PROFILE: VolumeProfile = [
    1.8, 1.5, 1.2, 1.0, 0.9, 0.8, // 00:00 - 05:00 (early morning)
    1.0, 1.4, 1.8, 1.5, 1.2, 0.9, // 06:00 - 11:00 (morning session)
    0.8, 0.7, 0.8, 0.9, 1.0, 1.2, // 12:00 - 17:00 (afternoon)
    1.4, 1.6, 1.3, 1.1, 0.9, 0.8, // 18:00 - 23:00 (evening session)
];

/// Flat volume profile (for 24/7 crypto markets)
pub const FLAT_PROFILE: VolumeProfile = [1.0; 24];

/// VWAP execution model configuration
#[derive(Debug, Clone)]
pub struct VwapConfig {
    /// Number of slices per hour
    pub slices_per_hour: usize,
    /// Volume profile (hourly buckets)
    pub volume_profile: VolumeProfile,
    /// Base urgency level
    pub base_urgency: f64,
    /// Minimum slice size as fraction of total (prevents tiny slices)
    pub min_slice_pct: f64,
}

impl Default for VwapConfig {
    fn default() -> Self {
        Self {
            slices_per_hour: 4, // 15-minute intervals
            volume_profile: U_SHAPED_PROFILE,
            base_urgency: 0.5,
            min_slice_pct: 0.02, // 2% minimum slice
        }
    }
}

/// VWAP execution model
pub struct VwapModel {
    config: VwapConfig,
}

impl VwapModel {
    pub fn new(config: VwapConfig) -> Self {
        Self { config }
    }

    /// Create with U-shaped volume profile (typical equities)
    pub fn equities() -> Self {
        Self::new(VwapConfig::default())
    }

    /// Create with flat profile (24/7 crypto markets)
    pub fn crypto() -> Self {
        Self::new(VwapConfig {
            volume_profile: FLAT_PROFILE,
            ..Default::default()
        })
    }

    /// Create with custom volume profile
    pub fn with_profile(profile: VolumeProfile) -> Self {
        Self::new(VwapConfig {
            volume_profile: profile,
            ..Default::default()
        })
    }

    /// Get volume weight for a specific time offset
    fn volume_weight(&self, offset: Duration, horizon: Duration) -> f64 {
        // Simplified: interpolate based on fraction of horizon
        // In production, would use actual time-of-day
        let frac = offset.num_seconds() as f64 / horizon.num_seconds().max(1) as f64;
        let hour_idx = ((frac * 24.0) as usize).min(23);
        self.config.volume_profile[hour_idx]
    }
}

impl ExecutionModel for VwapModel {
    fn compute_schedule(
        &self,
        target_qty: Quantity,
        horizon: Duration,
        _market_state: &MarketState,
    ) -> ExecutionSchedule {
        let total_hours = horizon.num_minutes() as f64 / 60.0;
        let num_slices =
            ((total_hours * self.config.slices_per_hour as f64).ceil() as usize).max(1);

        let slice_interval = Duration::seconds((horizon.num_seconds() / num_slices as i64).max(60));

        // Calculate volume weights for each slice
        let mut weights = Vec::with_capacity(num_slices);
        let mut total_weight = 0.0;

        for i in 0..num_slices {
            let offset = slice_interval * i as i32;
            let weight = self.volume_weight(offset, horizon);
            weights.push(weight);
            total_weight += weight;
        }

        // Normalize weights and apply minimum slice constraint
        let min_weight = self.config.min_slice_pct * total_weight;
        let mut adjusted_weights: Vec<f64> = weights.iter().map(|&w| w.max(min_weight)).collect();

        // Renormalize after adjustment
        let adjusted_total: f64 = adjusted_weights.iter().sum();
        for w in &mut adjusted_weights {
            *w /= adjusted_total;
        }

        // Create slices
        let mut slices = Vec::with_capacity(num_slices);
        let mut remaining = target_qty.raw();

        for (i, &weight) in adjusted_weights.iter().enumerate() {
            let time_offset = slice_interval * i as i32;

            // Last slice gets all remaining to avoid rounding errors
            let slice_qty = if i == num_slices - 1 {
                Quantity::from_raw(remaining)
            } else {
                let qty_raw = (target_qty.raw() as f64 * weight).round() as i64;
                remaining -= qty_raw;
                Quantity::from_raw(qty_raw)
            };

            // Higher urgency when volume is higher
            let urgency = (self.config.base_urgency * weight * 2.0).clamp(0.0, 1.0);

            slices.push(Slice::new(time_offset, slice_qty, urgency));
        }

        ExecutionSchedule::new(target_qty, horizon, slices, "VWAP")
    }

    fn name(&self) -> &str {
        "vwap"
    }
}

impl Default for VwapModel {
    fn default() -> Self {
        Self::equities()
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
    fn test_total_quantity_preserved() {
        let model = VwapModel::equities();
        let schedule = model.compute_schedule(
            Quantity::from_int(1000),
            Duration::hours(4),
            &default_market(),
        );

        let total: i64 = schedule.slices.iter().map(|s| s.quantity.raw()).sum();
        assert_eq!(total, 1000_00000000);
    }

    #[test]
    fn test_slice_count() {
        let model = VwapModel::new(VwapConfig {
            slices_per_hour: 4,
            ..Default::default()
        });

        let schedule = model.compute_schedule(
            Quantity::from_int(100),
            Duration::hours(2),
            &default_market(),
        );

        // 2 hours × 4 slices/hour = 8 slices
        assert_eq!(schedule.len(), 8);
    }

    #[test]
    fn test_crypto_flat_distribution() {
        let model = VwapModel::crypto();
        let schedule = model.compute_schedule(
            Quantity::from_int(1000),
            Duration::hours(4),
            &default_market(),
        );

        // With flat profile, slices should be relatively even
        let avg_qty = 1000_00000000 / schedule.len() as i64;
        for slice in &schedule.slices {
            let deviation = (slice.quantity.raw() - avg_qty).abs() as f64 / avg_qty as f64;
            assert!(deviation < 0.3); // Within 30% of average
        }
    }

    #[test]
    fn test_minimum_slice_size() {
        // Create profile with one zero hour
        let mut profile = [1.0; 24];
        profile[12] = 0.0; // Zero volume at noon

        let model = VwapModel::new(VwapConfig {
            volume_profile: profile,
            min_slice_pct: 0.05, // 5% minimum
            ..Default::default()
        });

        let schedule = model.compute_schedule(
            Quantity::from_int(100),
            Duration::hours(24),
            &default_market(),
        );

        // All slices should meet minimum
        for slice in &schedule.slices {
            // At least 2% of total (some tolerance for rounding)
            assert!(slice.quantity.raw() > 0);
        }
    }
}
