//! Toxicity Detection Types
//!
//! VPIN and order flow toxicity metrics.

use serde::{Deserialize, Serialize};

/// Toxicity level classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToxicityLevel {
    /// Normal balanced flow (~0.3)
    Normal,
    /// Some informed activity detected (~0.5)
    Elevated,
    /// Significant toxicity (~0.7)
    High,
    /// Extreme toxicity, consider pulling quotes (~0.9+)
    Extreme,
}

impl ToxicityLevel {
    /// Get spread multiplier for this toxicity level
    pub fn spread_multiplier(&self) -> f64 {
        match self {
            ToxicityLevel::Normal => 1.0,
            ToxicityLevel::Elevated => 1.3,
            ToxicityLevel::High => 1.8,
            ToxicityLevel::Extreme => 3.0,
        }
    }

    /// Get depth multiplier for this toxicity level
    pub fn depth_multiplier(&self) -> f64 {
        match self {
            ToxicityLevel::Normal => 1.0,
            ToxicityLevel::Elevated => 0.8,
            ToxicityLevel::High => 0.5,
            ToxicityLevel::Extreme => 0.2,
        }
    }

    /// Should we pull quotes?
    pub fn should_pull(&self) -> bool {
        matches!(self, ToxicityLevel::Extreme)
    }
}

impl Default for ToxicityLevel {
    fn default() -> Self {
        ToxicityLevel::Normal
    }
}

/// VPIN and toxicity metrics
///
/// From docs Section 8: VPIN measures order flow imbalance using volume buckets
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToxicityMetrics {
    /// Volume-synchronized Probability of Informed Trading
    /// Range: [0.0, 1.0]
    /// ~0.3 = Normal, ~0.5 = Elevated, ~0.7+ = High, ~0.9+ = Extreme
    pub vpin: f64,

    /// Order Flow Imbalance
    /// (Bid improvement - Bid deterioration) - (Ask improvement - Ask deterioration)
    /// Range: [-1.0, 1.0]
    pub ofi: f64,

    /// Derived toxicity level
    pub level: ToxicityLevel,
}

impl ToxicityMetrics {
    /// Create from VPIN and OFI values
    pub fn new(vpin: f64, ofi: f64) -> Self {
        let level = Self::classify_level(vpin);
        Self { vpin, ofi, level }
    }

    /// Classify VPIN into toxicity level
    fn classify_level(vpin: f64) -> ToxicityLevel {
        if vpin >= 0.85 {
            ToxicityLevel::Extreme
        } else if vpin >= 0.65 {
            ToxicityLevel::High
        } else if vpin >= 0.45 {
            ToxicityLevel::Elevated
        } else {
            ToxicityLevel::Normal
        }
    }

    /// Check if there's directional pressure
    pub fn has_directional_pressure(&self) -> bool {
        self.ofi.abs() > 0.3
    }

    /// Get the direction of flow (positive = buying pressure)
    pub fn flow_direction(&self) -> f64 {
        self.ofi
    }

    /// Combined toxicity score (0.0 - 1.0)
    pub fn combined_score(&self) -> f64 {
        // Weight VPIN more heavily
        (self.vpin * 0.7 + self.ofi.abs() * 0.3).min(1.0)
    }
}

impl Default for ToxicityMetrics {
    fn default() -> Self {
        Self {
            vpin: 0.3,
            ofi: 0.0,
            level: ToxicityLevel::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toxicity_classification() {
        let normal = ToxicityMetrics::new(0.25, 0.0);
        assert_eq!(normal.level, ToxicityLevel::Normal);

        let elevated = ToxicityMetrics::new(0.5, 0.1);
        assert_eq!(elevated.level, ToxicityLevel::Elevated);

        let high = ToxicityMetrics::new(0.75, 0.2);
        assert_eq!(high.level, ToxicityLevel::High);

        let extreme = ToxicityMetrics::new(0.9, 0.5);
        assert_eq!(extreme.level, ToxicityLevel::Extreme);
    }

    #[test]
    fn test_directional_pressure() {
        let neutral = ToxicityMetrics::new(0.3, 0.1);
        assert!(!neutral.has_directional_pressure());

        let directional = ToxicityMetrics::new(0.3, 0.5);
        assert!(directional.has_directional_pressure());
    }

    #[test]
    fn test_multipliers() {
        assert!((ToxicityLevel::Normal.spread_multiplier() - 1.0).abs() < 0.01);
        assert!(ToxicityLevel::Extreme.spread_multiplier() > 2.5);
        assert!(ToxicityLevel::Extreme.depth_multiplier() < 0.3);
    }
}
