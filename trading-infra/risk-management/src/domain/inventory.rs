//! Inventory Management Types
//!
//! For market maker position and risk tracking.

use serde::{Deserialize, Serialize};
use trading_core::Quantity;

/// Market maker inventory state
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    /// Current position (positive = long, negative = short)
    pub position: i64,
    /// Maximum allowed position (absolute value)
    pub max_position: i64,
    /// Target position (usually 0 for mean reversion)
    pub target: i64,
}

impl Inventory {
    pub fn new(max_position: i64) -> Self {
        Self {
            position: 0,
            max_position: max_position.abs(),
            target: 0,
        }
    }

    /// Create with current position
    pub fn with_position(position: i64, max_position: i64) -> Self {
        Self {
            position,
            max_position: max_position.abs(),
            target: 0,
        }
    }

    /// Get position as Quantity
    pub fn as_quantity(&self) -> Quantity {
        Quantity::from_raw(self.position.abs())
    }

    /// Is position long?
    pub fn is_long(&self) -> bool {
        self.position > 0
    }

    /// Is position short?
    pub fn is_short(&self) -> bool {
        self.position < 0
    }

    /// Get inventory ratio: position / max_position
    /// Range: [-1.0, 1.0]
    pub fn ratio(&self) -> f64 {
        if self.max_position == 0 {
            return 0.0;
        }
        self.position as f64 / self.max_position as f64
    }

    /// Get absolute inventory ratio
    pub fn abs_ratio(&self) -> f64 {
        self.ratio().abs()
    }

    /// Distance from target as ratio
    pub fn target_distance(&self) -> f64 {
        if self.max_position == 0 {
            return 0.0;
        }
        (self.position - self.target) as f64 / self.max_position as f64
    }

    /// Check if position is at limit
    pub fn at_limit(&self) -> bool {
        self.position.abs() >= self.max_position
    }

    /// Check if can buy more
    pub fn can_buy(&self) -> bool {
        self.position < self.max_position
    }

    /// Check if can sell more
    pub fn can_sell(&self) -> bool {
        self.position > -self.max_position
    }

    /// Update position after trade
    pub fn apply_trade(&mut self, signed_quantity: i64) {
        self.position += signed_quantity;
        // Clamp to limits
        self.position = self.position.clamp(-self.max_position, self.max_position);
    }

    /// Get urgency to reduce inventory (for skewing)
    /// Higher when closer to limits
    pub fn reduction_urgency(&self) -> f64 {
        let abs_ratio = self.abs_ratio();
        if abs_ratio < 0.5 {
            0.0
        } else {
            (abs_ratio - 0.5) * 2.0 // 0 at 50%, 1 at 100%
        }
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new(1000_00000000) // Default 1000 units max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inventory_ratio() {
        let inv = Inventory::with_position(500, 1000);
        assert!((inv.ratio() - 0.5).abs() < 0.01);

        let inv_short = Inventory::with_position(-300, 1000);
        assert!((inv_short.ratio() - (-0.3)).abs() < 0.01);
    }

    #[test]
    fn test_at_limit() {
        let mut inv = Inventory::new(100);
        assert!(!inv.at_limit());

        inv.position = 100;
        assert!(inv.at_limit());
        assert!(!inv.can_buy());
        assert!(inv.can_sell());

        inv.position = -100;
        assert!(inv.at_limit());
        assert!(inv.can_buy());
        assert!(!inv.can_sell());
    }

    #[test]
    fn test_apply_trade() {
        let mut inv = Inventory::new(100);
        inv.apply_trade(50);
        assert_eq!(inv.position, 50);

        // Should clamp at limit
        inv.apply_trade(100);
        assert_eq!(inv.position, 100);
    }

    #[test]
    fn test_reduction_urgency() {
        let low = Inventory::with_position(30, 100);
        assert!(low.reduction_urgency() < 0.01);

        let high = Inventory::with_position(80, 100);
        assert!(high.reduction_urgency() > 0.5);
    }
}
