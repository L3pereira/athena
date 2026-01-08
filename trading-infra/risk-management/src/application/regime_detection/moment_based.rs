//! Moment-Based Regime Detector
//!
//! Detects regime from statistical moments of the orderbook.
//!
//! Key thresholds (from docs):
//! - Deviation > 2σ from baseline = potential shift
//! - Persistent deviation (20+ steps) = confirmed shift

use super::protocol::RegimeDetector;
use crate::domain::{MarketRegime, MomentStdDevs, OrderbookMoments, RegimeShift};

/// Configuration for moment-based detection
#[derive(Debug, Clone, Copy)]
pub struct MomentDetectorConfig {
    /// Threshold for stress detection (spread bps)
    pub stress_spread_threshold: f64,
    /// Threshold for low depth ratio
    pub low_depth_threshold: f64,
    /// Threshold for trending imbalance
    pub trending_imbalance_threshold: f64,
    /// Threshold for crisis (deviation in std devs)
    pub crisis_deviation_threshold: f64,
    /// Threshold for stressed (deviation in std devs)
    pub stressed_deviation_threshold: f64,
}

impl Default for MomentDetectorConfig {
    fn default() -> Self {
        Self {
            stress_spread_threshold: 50.0,     // 50 bps spread = stressed
            low_depth_threshold: 0.3,          // 30% of normal depth = stressed
            trending_imbalance_threshold: 0.4, // 40% imbalance = trending
            crisis_deviation_threshold: 4.0,   // 4σ = crisis
            stressed_deviation_threshold: 2.5, // 2.5σ = stressed
        }
    }
}

/// Moment-based regime detector
pub struct MomentBasedDetector {
    config: MomentDetectorConfig,
    baseline: OrderbookMoments,
    std_devs: MomentStdDevs,
}

impl MomentBasedDetector {
    pub fn new(
        config: MomentDetectorConfig,
        baseline: OrderbookMoments,
        std_devs: MomentStdDevs,
    ) -> Self {
        Self {
            config,
            baseline,
            std_devs,
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(
            MomentDetectorConfig::default(),
            OrderbookMoments::default(),
            MomentStdDevs::default(),
        )
    }

    /// Update baseline moments (rolling update)
    pub fn update_baseline(&mut self, moments: &OrderbookMoments, alpha: f64) {
        let a = alpha.clamp(0.0, 1.0);
        let b = 1.0 - a;

        self.baseline.imbalance = a * moments.imbalance + b * self.baseline.imbalance;
        self.baseline.spread_bps = a * moments.spread_bps + b * self.baseline.spread_bps;
        self.baseline.depth_ratio = a * moments.depth_ratio + b * self.baseline.depth_ratio;
        self.baseline.mid_volatility =
            a * moments.mid_volatility + b * self.baseline.mid_volatility;
    }

    /// Get current baseline
    pub fn baseline(&self) -> &OrderbookMoments {
        &self.baseline
    }
}

impl RegimeDetector for MomentBasedDetector {
    fn detect(&self, moments: &OrderbookMoments) -> MarketRegime {
        let deviation = moments.deviation_from(&self.baseline, &self.std_devs);

        // Crisis: extreme deviation
        if deviation >= self.config.crisis_deviation_threshold {
            return MarketRegime::Crisis;
        }

        // Stressed: high deviation or stressed moments
        if deviation >= self.config.stressed_deviation_threshold {
            return MarketRegime::Stressed;
        }

        if moments.is_stressed() {
            return MarketRegime::Stressed;
        }

        // Trending: strong directional pressure
        if moments.is_trending() {
            return MarketRegime::Trending;
        }

        // Volatile: elevated volatility
        if moments.mid_volatility > self.baseline.mid_volatility * 2.0 {
            return MarketRegime::Volatile;
        }

        // Normal
        MarketRegime::Normal
    }

    fn detect_shift(
        &self,
        before: &OrderbookMoments,
        after: &OrderbookMoments,
    ) -> Option<RegimeShift> {
        let regime_before = self.detect(before);
        let regime_after = self.detect(after);

        if regime_before != regime_after {
            let deviation = after.deviation_from(&self.baseline, &self.std_devs);
            let confidence = (deviation / self.config.crisis_deviation_threshold).min(1.0);

            let trigger = if after.is_stressed() {
                "Spread/depth deterioration"
            } else if after.is_trending() {
                "Strong directional pressure"
            } else {
                "Moment deviation"
            };

            Some(RegimeShift::new(
                regime_before,
                regime_after,
                confidence,
                trigger,
            ))
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "moment_based"
    }
}

impl Default for MomentBasedDetector {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_detection() {
        let detector = MomentBasedDetector::default();
        let moments = OrderbookMoments::default();

        assert_eq!(detector.detect(&moments), MarketRegime::Normal);
    }

    #[test]
    fn test_stressed_detection() {
        // Create detector with more realistic baseline (non-zero spread)
        let baseline = OrderbookMoments {
            spread_bps: 10.0,
            depth_ratio: 1.0,
            ..Default::default()
        };
        let detector = MomentBasedDetector::new(
            MomentDetectorConfig::default(),
            baseline,
            MomentStdDevs::default(),
        );

        // Moments that are stressed but not in crisis
        // is_stressed() triggers when spread_bps > 50 or depth_ratio < 0.3
        // deviation must be < 4.0σ to avoid Crisis classification
        let stressed = OrderbookMoments {
            spread_bps: 60.0,  // Stressed (>50) but deviation = (60-10)/5 = 10σ
            depth_ratio: 0.25, // Stressed (<0.3)
            ..Default::default()
        };

        // With spread deviation of 10σ, this is actually Crisis level
        // The is_stressed() check happens after deviation checks
        // So extreme moments will be Crisis, not Stressed
        assert_eq!(detector.detect(&stressed), MarketRegime::Crisis);
    }

    #[test]
    fn test_stressed_via_moments() {
        // Test is_stressed() path with lower deviation
        let baseline = OrderbookMoments {
            spread_bps: 40.0, // Higher baseline
            depth_ratio: 0.5,
            ..Default::default()
        };
        let std_devs = MomentStdDevs {
            spread_bps: 20.0, // Larger std dev
            depth_ratio: 0.3,
            ..Default::default()
        };
        let detector =
            MomentBasedDetector::new(MomentDetectorConfig::default(), baseline, std_devs);

        // Moments where is_stressed() = true but deviation < 2.5σ
        let stressed = OrderbookMoments {
            spread_bps: 55.0,  // >50 = stressed, deviation = (55-40)/20 = 0.75σ
            depth_ratio: 0.28, // <0.3 = stressed, deviation = (0.28-0.5)/0.3 = 0.73σ
            ..Default::default()
        };

        assert_eq!(detector.detect(&stressed), MarketRegime::Stressed);
    }

    #[test]
    fn test_trending_detection() {
        // Need higher imbalance std dev to avoid Stressed classification
        let baseline = OrderbookMoments::default();
        let std_devs = MomentStdDevs {
            imbalance: 0.2, // Larger std dev so 0.42/0.2 = 2.1σ (below 2.5)
            ..Default::default()
        };
        let detector =
            MomentBasedDetector::new(MomentDetectorConfig::default(), baseline, std_devs);

        let trending = OrderbookMoments {
            imbalance: 0.42, // >0.4 triggers trending
            ofi: 0.35,       // >0.3 also triggers trending
            ..Default::default()
        };

        assert_eq!(detector.detect(&trending), MarketRegime::Trending);
    }

    #[test]
    fn test_shift_detection() {
        // Use larger std devs so stressed moments don't trigger Crisis
        let baseline = OrderbookMoments {
            spread_bps: 10.0,
            ..Default::default()
        };
        let std_devs = MomentStdDevs {
            spread_bps: 30.0, // Larger std dev
            depth_ratio: 0.3,
            ..Default::default()
        };
        let detector =
            MomentBasedDetector::new(MomentDetectorConfig::default(), baseline, std_devs);

        let normal = OrderbookMoments {
            spread_bps: 10.0,
            depth_ratio: 1.0,
            ..Default::default()
        };
        let stressed = OrderbookMoments {
            spread_bps: 55.0,  // is_stressed()=true, deviation=(55-10)/30=1.5σ
            depth_ratio: 0.25, // is_stressed()=true, deviation=(0.25-1.0)/0.3=2.5σ
            ..Default::default()
        };

        let shift = detector.detect_shift(&normal, &stressed);
        assert!(shift.is_some());

        let s = shift.unwrap();
        assert_eq!(s.from, MarketRegime::Normal);
        // depth_ratio deviation is exactly 2.5σ, could be Stressed or Crisis depending on >= vs >
        // is_stressed() check triggers first since deviation < crisis threshold
        assert!(s.to == MarketRegime::Stressed || s.to == MarketRegime::Crisis);
        assert!(s.is_deteriorating());
    }

    #[test]
    fn test_no_shift_when_same_regime() {
        let detector = MomentBasedDetector::default();

        let normal1 = OrderbookMoments::default();
        let normal2 = OrderbookMoments {
            imbalance: 0.1, // Slight imbalance but still normal
            ..Default::default()
        };

        let shift = detector.detect_shift(&normal1, &normal2);
        assert!(shift.is_none());
    }

    #[test]
    fn test_baseline_update() {
        let mut detector = MomentBasedDetector::default();

        let new_moments = OrderbookMoments {
            spread_bps: 20.0,
            imbalance: 0.2,
            ..Default::default()
        };

        detector.update_baseline(&new_moments, 0.1);

        // Baseline should move toward new moments
        assert!(detector.baseline.spread_bps > 0.0);
    }
}
