//! Regime Shift Detector
//!
//! Detects persistent regime shifts (not just transient deviations).
//!
//! From docs: "Deviation persists (not transient)
//! - Check after 20+ steps
//! - If still deviated → confirmed shift"

use super::protocol::RegimeDetector;
use crate::domain::{MarketRegime, MomentStdDevs, OrderbookMoments, RegimeShift};

/// Configuration for shift detection
#[derive(Debug, Clone, Copy)]
pub struct ShiftDetectorConfig {
    /// Number of steps required to confirm a shift
    pub confirmation_steps: u32,
    /// Deviation threshold (in std devs) to start counting
    pub deviation_threshold: f64,
    /// Maximum steps before auto-reset
    pub max_steps: u32,
}

impl Default for ShiftDetectorConfig {
    fn default() -> Self {
        Self {
            confirmation_steps: 20,
            deviation_threshold: 2.0,
            max_steps: 100,
        }
    }
}

/// State for tracking potential regime shifts
#[derive(Debug, Clone, Default)]
struct ShiftState {
    /// Steps since deviation started
    steps_elevated: u32,
    /// Current suspected new regime
    suspected_regime: Option<MarketRegime>,
    /// Average deviation during this period
    avg_deviation: f64,
}

/// Regime shift detector with persistence confirmation
pub struct RegimeShiftDetector {
    config: ShiftDetectorConfig,
    baseline: OrderbookMoments,
    std_devs: MomentStdDevs,
    current_regime: MarketRegime,
    shift_state: ShiftState,
}

impl RegimeShiftDetector {
    pub fn new(
        config: ShiftDetectorConfig,
        baseline: OrderbookMoments,
        std_devs: MomentStdDevs,
    ) -> Self {
        Self {
            config,
            baseline,
            std_devs,
            current_regime: MarketRegime::Normal,
            shift_state: ShiftState::default(),
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(
            ShiftDetectorConfig::default(),
            OrderbookMoments::default(),
            MomentStdDevs::default(),
        )
    }

    /// Update with new moments, returns confirmed shift if any
    pub fn update(&mut self, moments: &OrderbookMoments) -> Option<RegimeShift> {
        let deviation = moments.deviation_from(&self.baseline, &self.std_devs);
        let detected_regime = self.classify_regime(moments, deviation);

        // If deviation is above threshold and regime differs
        if deviation >= self.config.deviation_threshold && detected_regime != self.current_regime {
            // Already tracking this regime?
            if self.shift_state.suspected_regime == Some(detected_regime) {
                self.shift_state.steps_elevated += 1;
                self.shift_state.avg_deviation = (self.shift_state.avg_deviation
                    * (self.shift_state.steps_elevated - 1) as f64
                    + deviation)
                    / self.shift_state.steps_elevated as f64;

                // Confirmed shift?
                if self.shift_state.steps_elevated >= self.config.confirmation_steps {
                    let shift = RegimeShift {
                        from: self.current_regime,
                        to: detected_regime,
                        confidence: (self.shift_state.avg_deviation / 4.0).min(1.0),
                        trigger: format!(
                            "Persistent deviation ({} steps, {:.1}σ avg)",
                            self.shift_state.steps_elevated, self.shift_state.avg_deviation
                        ),
                        persistence_steps: self.shift_state.steps_elevated,
                    };

                    // Apply the shift
                    self.current_regime = detected_regime;
                    self.shift_state = ShiftState::default();

                    return Some(shift);
                }
            } else {
                // Start tracking new potential shift
                self.shift_state = ShiftState {
                    steps_elevated: 1,
                    suspected_regime: Some(detected_regime),
                    avg_deviation: deviation,
                };
            }
        } else {
            // Deviation dropped or same regime - reset tracking
            if self.shift_state.steps_elevated > 0 {
                self.shift_state = ShiftState::default();
            }
        }

        // Auto-reset if max steps exceeded without confirmation
        if self.shift_state.steps_elevated >= self.config.max_steps {
            self.shift_state = ShiftState::default();
        }

        None
    }

    /// Get current confirmed regime
    pub fn current_regime(&self) -> MarketRegime {
        self.current_regime
    }

    /// Get steps in current potential shift
    pub fn pending_shift_steps(&self) -> u32 {
        self.shift_state.steps_elevated
    }

    /// Check if a shift is pending confirmation
    pub fn has_pending_shift(&self) -> bool {
        self.shift_state.suspected_regime.is_some()
    }

    /// Classify regime from moments
    fn classify_regime(&self, moments: &OrderbookMoments, deviation: f64) -> MarketRegime {
        if deviation >= 4.0 {
            MarketRegime::Crisis
        } else if deviation >= 2.5 || moments.is_stressed() {
            MarketRegime::Stressed
        } else if moments.is_trending() {
            MarketRegime::Trending
        } else if moments.mid_volatility > 0.05 {
            MarketRegime::Volatile
        } else {
            MarketRegime::Normal
        }
    }

    /// Update baseline (use carefully - affects all detection)
    pub fn update_baseline(&mut self, new_baseline: OrderbookMoments) {
        self.baseline = new_baseline;
    }
}

impl RegimeDetector for RegimeShiftDetector {
    fn detect(&self, moments: &OrderbookMoments) -> MarketRegime {
        let deviation = moments.deviation_from(&self.baseline, &self.std_devs);
        self.classify_regime(moments, deviation)
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
            Some(RegimeShift::new(
                regime_before,
                regime_after,
                (deviation / 4.0).min(1.0),
                "Instant shift (not confirmed)",
            ))
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "regime_shift_detector"
    }
}

impl Default for RegimeShiftDetector {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stressed moments with high deviation (will be classified as Crisis)
    #[allow(dead_code)]
    fn crisis_moments() -> OrderbookMoments {
        OrderbookMoments {
            spread_bps: 100.0, // deviation = 100/5 = 20σ → Crisis
            depth_ratio: 0.2,
            imbalance: 0.5,
            mid_volatility: 0.1,
            ..Default::default()
        }
    }

    /// Stressed moments with controlled deviation (Stressed, not Crisis)
    fn stressed_moments_controlled() -> OrderbookMoments {
        OrderbookMoments {
            spread_bps: 55.0,  // is_stressed() = true (>50)
            depth_ratio: 0.25, // is_stressed() = true (<0.3)
            imbalance: 0.1,
            mid_volatility: 0.02,
            ..Default::default()
        }
    }

    /// Larger std devs so stressed_moments produce < 4σ deviation
    fn large_std_devs() -> MomentStdDevs {
        MomentStdDevs {
            spread_bps: 30.0, // 55/30 = 1.8σ
            depth_ratio: 0.4, // (0.25-1.0)/0.4 = 1.9σ
            imbalance: 0.2,
            mid_volatility: 0.05,
        }
    }

    #[test]
    fn test_single_deviation_no_shift() {
        let mut detector = RegimeShiftDetector::new(
            ShiftDetectorConfig {
                confirmation_steps: 5,
                deviation_threshold: 1.0, // Lower threshold for testing
                ..Default::default()
            },
            OrderbookMoments::default(),
            large_std_devs(),
        );

        // Single stressed reading should not confirm shift
        let shift = detector.update(&stressed_moments_controlled());
        assert!(shift.is_none());
        assert!(detector.has_pending_shift());
        assert_eq!(detector.pending_shift_steps(), 1);
    }

    #[test]
    fn test_persistent_deviation_confirms_shift() {
        let mut detector = RegimeShiftDetector::new(
            ShiftDetectorConfig {
                confirmation_steps: 3,
                deviation_threshold: 1.0,
                ..Default::default()
            },
            OrderbookMoments::default(),
            large_std_devs(),
        );

        let stressed = stressed_moments_controlled();

        // First update
        assert!(detector.update(&stressed).is_none());
        assert_eq!(detector.pending_shift_steps(), 1);

        // Second update
        assert!(detector.update(&stressed).is_none());
        assert_eq!(detector.pending_shift_steps(), 2);

        // Third update - should confirm
        let shift = detector.update(&stressed);
        assert!(shift.is_some());

        let s = shift.unwrap();
        assert_eq!(s.from, MarketRegime::Normal);
        assert_eq!(s.to, MarketRegime::Stressed);
        assert_eq!(s.persistence_steps, 3);
    }

    #[test]
    fn test_reset_on_normal() {
        let mut detector = RegimeShiftDetector::new(
            ShiftDetectorConfig {
                confirmation_steps: 5,
                deviation_threshold: 1.0,
                ..Default::default()
            },
            OrderbookMoments::default(),
            large_std_devs(),
        );

        // Start tracking stressed
        detector.update(&stressed_moments_controlled());
        detector.update(&stressed_moments_controlled());
        assert_eq!(detector.pending_shift_steps(), 2);

        // Back to normal - should reset
        detector.update(&OrderbookMoments::default());
        assert!(!detector.has_pending_shift());
        assert_eq!(detector.pending_shift_steps(), 0);
    }

    #[test]
    fn test_current_regime_updates() {
        let mut detector = RegimeShiftDetector::new(
            ShiftDetectorConfig {
                confirmation_steps: 2,
                deviation_threshold: 1.0,
                ..Default::default()
            },
            OrderbookMoments::default(),
            large_std_devs(),
        );

        assert_eq!(detector.current_regime(), MarketRegime::Normal);

        detector.update(&stressed_moments_controlled());
        detector.update(&stressed_moments_controlled());

        assert_eq!(detector.current_regime(), MarketRegime::Stressed);
    }
}
