//! Impact Types
//!
//! Domain models for representing market impact estimates.

use serde::{Deserialize, Serialize};

/// Simple impact estimate (price movement in basis points)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Impact {
    /// Price impact in basis points (1 bp = 0.01%)
    pub price_bps: f64,
    /// Whether this is a permanent or transient impact
    pub is_permanent: bool,
}

impl Impact {
    pub const ZERO: Impact = Impact {
        price_bps: 0.0,
        is_permanent: false,
    };

    pub fn permanent(price_bps: f64) -> Self {
        Self {
            price_bps,
            is_permanent: true,
        }
    }

    pub fn transient(price_bps: f64) -> Self {
        Self {
            price_bps,
            is_permanent: false,
        }
    }
}

impl Default for Impact {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Full L2 structure impact (from docs Section 3)
///
/// Captures impact across 5 dimensions:
/// - Price impact: Traditional price movement
/// - Spread impact: How much spread widens
/// - Depth impact: How much depth is consumed
/// - Volatility impact: Short-term vol increase
/// - Regime shift probability: P(structure changes permanently)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FullImpact {
    /// Traditional price move in basis points
    pub price_impact_bps: f64,

    /// How much spread widens (percentage)
    /// Spread_after = Spread_before × (1 + spread_impact_pct)
    pub spread_impact_pct: f64,

    /// How much depth consumed (percentage of visible depth)
    pub depth_impact_pct: f64,

    /// Short-term volatility increase (percentage)
    /// Vol_after = Vol_before × (1 + volatility_impact_pct)
    pub volatility_impact_pct: f64,

    /// Time for orderbook to recover (in seconds)
    /// Depth(t) = Depth_final - (Depth_consumed × e^(-t/half_life))
    pub recovery_half_life_secs: f64,

    /// Probability that this trade shifts the market regime permanently
    /// P(regime_shift) = σ(k × (depth_ratio - threshold))
    pub regime_shift_prob: f64,
}

impl FullImpact {
    pub const ZERO: FullImpact = FullImpact {
        price_impact_bps: 0.0,
        spread_impact_pct: 0.0,
        depth_impact_pct: 0.0,
        volatility_impact_pct: 0.0,
        recovery_half_life_secs: 0.0,
        regime_shift_prob: 0.0,
    };

    /// Total cost estimate (simple aggregation)
    pub fn total_cost_bps(&self) -> f64 {
        // Price impact + half spread widening + volatility cost
        self.price_impact_bps
            + self.spread_impact_pct * 50.0 // Convert spread % to bps estimate
            + self.volatility_impact_pct * 10.0 // Volatility cost multiplier
    }

    /// Check if this impact is likely to cause a regime shift
    pub fn is_regime_shifting(&self) -> bool {
        self.regime_shift_prob > 0.3
    }

    /// Compute a safe scaling factor to avoid regime shift
    /// Returns a multiplier (0..1) to reduce order size
    pub fn safe_size_multiplier(&self) -> f64 {
        if self.regime_shift_prob <= 0.1 {
            1.0
        } else {
            // Scale down: target 10% regime shift probability
            (0.1 / self.regime_shift_prob).min(1.0)
        }
    }
}

impl Default for FullImpact {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_impact_types() {
        let permanent = Impact::permanent(10.0);
        assert!(permanent.is_permanent);
        assert!((permanent.price_bps - 10.0).abs() < 0.001);

        let transient = Impact::transient(5.0);
        assert!(!transient.is_permanent);
    }

    #[test]
    fn test_full_impact_regime_shift() {
        let low_impact = FullImpact {
            regime_shift_prob: 0.1,
            ..Default::default()
        };
        assert!(!low_impact.is_regime_shifting());

        let high_impact = FullImpact {
            regime_shift_prob: 0.5,
            ..Default::default()
        };
        assert!(high_impact.is_regime_shifting());
    }

    #[test]
    fn test_safe_size_multiplier() {
        let low = FullImpact {
            regime_shift_prob: 0.05,
            ..Default::default()
        };
        assert!((low.safe_size_multiplier() - 1.0).abs() < 0.001);

        let high = FullImpact {
            regime_shift_prob: 0.5,
            ..Default::default()
        };
        assert!((high.safe_size_multiplier() - 0.2).abs() < 0.001);
    }
}
