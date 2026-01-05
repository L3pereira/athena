//! Market structure state assessment
//!
//! Provides liquidity scoring and stress detection from orderbook moments.

use super::OrderbookMoments;
use serde::{Deserialize, Serialize};

/// Current state of market structure
///
/// Computed from orderbook moments and optional flow toxicity (VPIN).
/// Used by agents to adjust behavior based on market conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStructureState {
    /// Liquidity score [0, 1]: higher = more liquid
    pub liquidity_score: f64,
    /// Stress level [0, 1]: higher = more stressed
    pub stress_level: f64,
    /// Is the market currently stressed? (stress > threshold)
    pub is_stressed: bool,
    /// Current detected regime index (0=normal, 1=volatile, 2=trending)
    pub regime_index: u8,
}

impl MarketStructureState {
    /// Stress threshold for is_stressed flag
    const STRESS_THRESHOLD: f64 = 0.5;

    /// Compute market structure state from orderbook moments
    ///
    /// # Arguments
    /// * `moments` - Current orderbook statistical moments
    /// * `vpin` - Optional Volume-synchronized Probability of Informed Trading [0, 1]
    /// * `regime_index` - Current regime (0=normal, 1=volatile, 2=trending)
    pub fn from_orderbook(moments: &OrderbookMoments, vpin: Option<f64>, regime_index: u8) -> Self {
        let liquidity_score = Self::compute_liquidity(moments);
        let stress_level = Self::compute_stress(moments, vpin);

        Self {
            liquidity_score,
            stress_level,
            is_stressed: stress_level > Self::STRESS_THRESHOLD,
            regime_index,
        }
    }

    /// Compute liquidity score from moments
    ///
    /// Components:
    /// - Tighter spread = more liquid
    /// - Deeper book = more liquid
    /// - Balanced imbalance = more liquid
    fn compute_liquidity(moments: &OrderbookMoments) -> f64 {
        // Spread component: 1 bps = 1.0, 50 bps = 0.0 (inverse, clamped)
        let spread_score = (1.0 - moments.spread_mean_bps / 50.0).clamp(0.0, 1.0);

        // Depth component: log scale, normalized
        let total_depth: f64 = moments.depth_mean.iter().sum();
        let depth_score = (total_depth.ln() / 10.0).clamp(0.0, 1.0);

        // Imbalance component: 0 imbalance = 1.0, high imbalance = lower
        let imbalance_score = 1.0 - moments.imbalance_mean.abs();

        // Weighted combination
        0.4 * spread_score + 0.4 * depth_score + 0.2 * imbalance_score
    }

    /// Compute stress level from moments and VPIN
    ///
    /// Components:
    /// - Wide spread = more stress
    /// - High imbalance variance = more stress
    /// - High VPIN (if provided) = more stress
    fn compute_stress(moments: &OrderbookMoments, vpin: Option<f64>) -> f64 {
        // Spread stress: wider = more stressed
        let spread_stress = (moments.spread_mean_bps / 50.0).clamp(0.0, 1.0);

        // Imbalance variance stress
        let imbalance_stress = (moments.imbalance_var * 10.0).clamp(0.0, 1.0);

        // VPIN stress (if provided)
        let vpin_stress = vpin.unwrap_or(0.3);

        // Weighted combination
        0.3 * spread_stress + 0.2 * imbalance_stress + 0.5 * vpin_stress
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_regime_not_stressed() {
        let moments = OrderbookMoments::default_normal();
        let state = MarketStructureState::from_orderbook(&moments, None, 0);

        assert!(state.liquidity_score > 0.5);
        assert!(state.stress_level < 0.5);
        assert!(!state.is_stressed);
    }

    #[test]
    fn test_volatile_regime_more_stressed() {
        let normal = OrderbookMoments::default_normal();
        let volatile = OrderbookMoments::default_volatile();

        let normal_state = MarketStructureState::from_orderbook(&normal, None, 0);
        let volatile_state = MarketStructureState::from_orderbook(&volatile, None, 1);

        assert!(volatile_state.stress_level > normal_state.stress_level);
        assert!(volatile_state.liquidity_score < normal_state.liquidity_score);
    }

    #[test]
    fn test_high_vpin_increases_stress() {
        let moments = OrderbookMoments::default_normal();

        let low_vpin = MarketStructureState::from_orderbook(&moments, Some(0.2), 0);
        let high_vpin = MarketStructureState::from_orderbook(&moments, Some(0.8), 0);

        assert!(high_vpin.stress_level > low_vpin.stress_level);
    }
}
