//! Execution Schedule Types
//!
//! Domain models for execution planning and slicing.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use trading_core::Quantity;

/// A single execution slice in a schedule
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Slice {
    /// Time offset from start when this slice should execute
    pub time_offset: Duration,
    /// Target quantity for this slice
    pub quantity: Quantity,
    /// Urgency level (0.0 = passive, 1.0 = aggressive)
    pub urgency: f64,
}

impl Slice {
    pub fn new(time_offset: Duration, quantity: Quantity, urgency: f64) -> Self {
        Self {
            time_offset,
            quantity,
            urgency: urgency.clamp(0.0, 1.0),
        }
    }
}

/// Execution schedule containing ordered slices
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionSchedule {
    /// Total target quantity
    pub total_quantity: Quantity,
    /// Total execution horizon
    pub horizon: Duration,
    /// Ordered list of slices
    pub slices: Vec<Slice>,
    /// Schedule creation time
    pub created_at: DateTime<Utc>,
    /// Description of the schedule (e.g., "TWAP", "VWAP", "IS")
    pub model_name: String,
}

impl ExecutionSchedule {
    pub fn new(
        total_quantity: Quantity,
        horizon: Duration,
        slices: Vec<Slice>,
        model_name: impl Into<String>,
    ) -> Self {
        Self {
            total_quantity,
            horizon,
            slices,
            created_at: Utc::now(),
            model_name: model_name.into(),
        }
    }

    /// Get the number of slices
    pub fn len(&self) -> usize {
        self.slices.len()
    }

    /// Check if schedule is empty
    pub fn is_empty(&self) -> bool {
        self.slices.is_empty()
    }

    /// Get the slice at a given index
    pub fn get(&self, index: usize) -> Option<&Slice> {
        self.slices.get(index)
    }

    /// Get the slice for a given time offset
    pub fn slice_at_time(&self, elapsed: Duration) -> Option<&Slice> {
        self.slices.iter().find(|s| s.time_offset >= elapsed)
    }

    /// Calculate remaining quantity from a given slice index
    pub fn remaining_quantity(&self, from_slice: usize) -> Quantity {
        self.slices
            .iter()
            .skip(from_slice)
            .fold(Quantity::ZERO, |acc, s| acc + s.quantity)
    }

    /// Get progress (0.0 to 1.0) at a given slice index
    pub fn progress(&self, completed_slices: usize) -> f64 {
        if self.slices.is_empty() {
            return 1.0;
        }
        completed_slices as f64 / self.slices.len() as f64
    }
}

/// Adjustment signal for adaptive execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Adjustment {
    /// Continue with current pace
    Maintain,
    /// Speed up execution (behind schedule or favorable conditions)
    MoreAggressive,
    /// Slow down execution (ahead of schedule or adverse conditions)
    LessAggressive,
    /// Pause execution (very adverse conditions)
    Pause,
    /// Skip this slice entirely
    Skip,
}

impl Adjustment {
    /// Convert to urgency multiplier
    pub fn urgency_multiplier(&self) -> f64 {
        match self {
            Adjustment::Maintain => 1.0,
            Adjustment::MoreAggressive => 1.5,
            Adjustment::LessAggressive => 0.5,
            Adjustment::Pause => 0.0,
            Adjustment::Skip => 0.0,
        }
    }

    /// Check if execution should continue
    pub fn should_execute(&self) -> bool {
        !matches!(self, Adjustment::Pause | Adjustment::Skip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_creation() {
        let slice = Slice::new(Duration::seconds(10), Quantity::from_int(100), 0.5);
        assert_eq!(slice.quantity.raw(), 100_00000000);
        assert!((slice.urgency - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_urgency_clamping() {
        let high = Slice::new(Duration::zero(), Quantity::from_int(1), 1.5);
        assert!((high.urgency - 1.0).abs() < 0.001);

        let low = Slice::new(Duration::zero(), Quantity::from_int(1), -0.5);
        assert!(low.urgency.abs() < 0.001);
    }

    #[test]
    fn test_schedule_progress() {
        let schedule = ExecutionSchedule::new(
            Quantity::from_int(1000),
            Duration::minutes(10),
            vec![
                Slice::new(Duration::minutes(0), Quantity::from_int(250), 0.5),
                Slice::new(Duration::minutes(2), Quantity::from_int(250), 0.5),
                Slice::new(Duration::minutes(5), Quantity::from_int(250), 0.5),
                Slice::new(Duration::minutes(8), Quantity::from_int(250), 0.5),
            ],
            "TWAP",
        );

        assert_eq!(schedule.len(), 4);
        assert!((schedule.progress(0) - 0.0).abs() < 0.001);
        assert!((schedule.progress(2) - 0.5).abs() < 0.001);
        assert!((schedule.progress(4) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_remaining_quantity() {
        let schedule = ExecutionSchedule::new(
            Quantity::from_int(1000),
            Duration::minutes(10),
            vec![
                Slice::new(Duration::minutes(0), Quantity::from_int(250), 0.5),
                Slice::new(Duration::minutes(2), Quantity::from_int(250), 0.5),
                Slice::new(Duration::minutes(5), Quantity::from_int(250), 0.5),
                Slice::new(Duration::minutes(8), Quantity::from_int(250), 0.5),
            ],
            "TWAP",
        );

        assert_eq!(schedule.remaining_quantity(0).raw(), 1000_00000000);
        assert_eq!(schedule.remaining_quantity(2).raw(), 500_00000000);
        assert_eq!(schedule.remaining_quantity(4).raw(), 0);
    }

    #[test]
    fn test_adjustment_urgency() {
        assert!((Adjustment::Maintain.urgency_multiplier() - 1.0).abs() < 0.001);
        assert!((Adjustment::MoreAggressive.urgency_multiplier() - 1.5).abs() < 0.001);
        assert!((Adjustment::Pause.urgency_multiplier() - 0.0).abs() < 0.001);
    }
}
