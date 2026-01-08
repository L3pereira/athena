//! Orderbook Moments
//!
//! Statistical moments of the orderbook for regime detection.

use serde::{Deserialize, Serialize};

/// Statistical moments of the orderbook
///
/// These capture the "shape" of the orderbook for regime detection:
/// - Level 1: Mean (imbalance, spread)
/// - Level 2: Variance (volatility of depth)
/// - Level 3: Skewness (asymmetry)
/// - Level 4: Kurtosis (tail behavior)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OrderbookMoments {
    /// Imbalance: (bid_depth - ask_depth) / (bid_depth + ask_depth)
    /// Range: [-1.0, 1.0]
    pub imbalance: f64,

    /// Spread in basis points
    pub spread_bps: f64,

    /// Depth ratio: current_depth / normal_depth
    pub depth_ratio: f64,

    /// Volatility of mid price (recent)
    pub mid_volatility: f64,

    /// Depth asymmetry: bid_depth / ask_depth
    pub depth_asymmetry: f64,

    /// Order flow imbalance (OFI)
    /// (Bid improvement - Bid deterioration) - (Ask improvement - Ask deterioration)
    pub ofi: f64,
}

impl OrderbookMoments {
    pub const ZERO: OrderbookMoments = OrderbookMoments {
        imbalance: 0.0,
        spread_bps: 0.0,
        depth_ratio: 1.0,
        mid_volatility: 0.0,
        depth_asymmetry: 1.0,
        ofi: 0.0,
    };

    /// Calculate deviation from baseline moments
    ///
    /// Returns the maximum deviation in standard deviations
    pub fn deviation_from(&self, baseline: &OrderbookMoments, std_devs: &MomentStdDevs) -> f64 {
        let deviations = [
            ((self.imbalance - baseline.imbalance) / std_devs.imbalance.max(0.01)).abs(),
            ((self.spread_bps - baseline.spread_bps) / std_devs.spread_bps.max(0.01)).abs(),
            ((self.depth_ratio - baseline.depth_ratio) / std_devs.depth_ratio.max(0.01)).abs(),
            ((self.mid_volatility - baseline.mid_volatility) / std_devs.mid_volatility.max(0.01))
                .abs(),
        ];

        deviations.iter().cloned().fold(0.0, f64::max)
    }

    /// Check if moments indicate a trending market
    pub fn is_trending(&self) -> bool {
        self.imbalance.abs() > 0.4 || self.ofi.abs() > 0.3
    }

    /// Check if moments indicate stress
    pub fn is_stressed(&self) -> bool {
        self.spread_bps > 50.0 || self.depth_ratio < 0.3
    }
}

impl Default for OrderbookMoments {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Standard deviations for each moment (for deviation calculations)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MomentStdDevs {
    pub imbalance: f64,
    pub spread_bps: f64,
    pub depth_ratio: f64,
    pub mid_volatility: f64,
}

impl Default for MomentStdDevs {
    fn default() -> Self {
        Self {
            imbalance: 0.15,
            spread_bps: 5.0,
            depth_ratio: 0.2,
            mid_volatility: 0.01,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deviation_calculation() {
        let baseline = OrderbookMoments {
            imbalance: 0.0,
            spread_bps: 5.0,
            depth_ratio: 1.0,
            mid_volatility: 0.02,
            depth_asymmetry: 1.0,
            ofi: 0.0,
        };

        let current = OrderbookMoments {
            imbalance: 0.3, // 2 std devs away (0.3 / 0.15 = 2)
            spread_bps: 5.0,
            depth_ratio: 1.0,
            mid_volatility: 0.02,
            depth_asymmetry: 1.0,
            ofi: 0.0,
        };

        let std_devs = MomentStdDevs::default();
        let deviation = current.deviation_from(&baseline, &std_devs);

        assert!((deviation - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_trending_detection() {
        let trending = OrderbookMoments {
            imbalance: 0.5, // Strong buy imbalance
            ofi: 0.4,
            ..Default::default()
        };
        assert!(trending.is_trending());

        let normal = OrderbookMoments {
            imbalance: 0.1,
            ofi: 0.1,
            ..Default::default()
        };
        assert!(!normal.is_trending());
    }

    #[test]
    fn test_stressed_detection() {
        let stressed = OrderbookMoments {
            spread_bps: 100.0,
            depth_ratio: 0.2,
            ..Default::default()
        };
        assert!(stressed.is_stressed());
    }
}
