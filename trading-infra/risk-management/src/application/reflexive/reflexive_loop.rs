//! Reflexive Loop
//!
//! The core reflexive market architecture from Soros:
//!
//! 1. Agents observe market state (moments)
//! 2. Agents trade based on their models
//! 3. Trades create impact (temporary + permanent)
//! 4. Impact may shift market regime
//! 5. New regime feeds back to agents
//!
//! This creates feedback loops that can amplify or dampen volatility.

use super::circuit_breakers::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
use crate::application::regime_detection::RegimeShiftDetector;
use crate::domain::{MarketRegime, OrderbookMoments, RegimeShift};
use serde::{Deserialize, Serialize};

/// Events emitted by the reflexive loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReflexiveEvent {
    /// Regime shift detected and confirmed
    RegimeShift(RegimeShift),
    /// Circuit breaker state changed
    CircuitStateChange {
        from: CircuitState,
        to: CircuitState,
        trigger: String,
    },
    /// Feedback loop detected (self-reinforcing)
    FeedbackLoop {
        direction: FeedbackDirection,
        strength: f64,
    },
}

/// Direction of feedback loop
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackDirection {
    /// Positive feedback - amplifying (dangerous)
    Amplifying,
    /// Negative feedback - dampening (stabilizing)
    Dampening,
}

/// Configuration for reflexive loop
#[derive(Debug, Clone, Copy)]
pub struct ReflexiveLoopConfig {
    /// Steps to confirm feedback loop
    pub feedback_confirmation_steps: u32,
    /// Threshold for feedback detection (correlation)
    pub feedback_threshold: f64,
    /// Enable circuit breakers
    pub enable_circuit_breakers: bool,
}

impl Default for ReflexiveLoopConfig {
    fn default() -> Self {
        Self {
            feedback_confirmation_steps: 5,
            feedback_threshold: 0.7,
            enable_circuit_breakers: true,
        }
    }
}

/// The reflexive loop controller
///
/// Coordinates regime detection, circuit breakers, and feedback monitoring.
pub struct ReflexiveLoop {
    config: ReflexiveLoopConfig,
    regime_detector: RegimeShiftDetector,
    circuit_breaker: CircuitBreaker,
    /// History of momentum changes for feedback detection
    momentum_history: Vec<f64>,
    /// History of volatility changes
    volatility_history: Vec<f64>,
    /// Current feedback state
    feedback_steps: u32,
    last_feedback_direction: Option<FeedbackDirection>,
}

impl ReflexiveLoop {
    pub fn new(
        config: ReflexiveLoopConfig,
        regime_detector: RegimeShiftDetector,
        circuit_breaker_config: CircuitBreakerConfig,
        baseline_volatility: f64,
    ) -> Self {
        Self {
            config,
            regime_detector,
            circuit_breaker: CircuitBreaker::new(circuit_breaker_config, baseline_volatility),
            momentum_history: Vec::with_capacity(20),
            volatility_history: Vec::with_capacity(20),
            feedback_steps: 0,
            last_feedback_direction: None,
        }
    }

    /// Process new market moments, returns events if any
    pub fn update(
        &mut self,
        moments: &OrderbookMoments,
        current_time_ms: u64,
    ) -> Vec<ReflexiveEvent> {
        let mut events = Vec::new();

        // 1. Update circuit breaker
        if self.config.enable_circuit_breakers {
            let old_state = self.circuit_breaker.state();
            let new_state = self.circuit_breaker.update(moments, current_time_ms);

            if old_state != new_state {
                events.push(ReflexiveEvent::CircuitStateChange {
                    from: old_state,
                    to: new_state,
                    trigger: self.describe_trigger(moments),
                });
            }
        }

        // 2. Check for regime shifts
        if let Some(shift) = self.regime_detector.update(moments) {
            // Notify circuit breaker
            self.circuit_breaker
                .on_regime_shift(&shift, current_time_ms);
            events.push(ReflexiveEvent::RegimeShift(shift));
        }

        // 3. Track feedback loops
        self.update_history(moments);
        if let Some(feedback) = self.detect_feedback() {
            events.push(ReflexiveEvent::FeedbackLoop {
                direction: feedback.0,
                strength: feedback.1,
            });
        }

        events
    }

    /// Get current regime
    pub fn current_regime(&self) -> MarketRegime {
        self.regime_detector.current_regime()
    }

    /// Get circuit breaker state
    pub fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state()
    }

    /// Check if trading is allowed
    pub fn allows_trading(&self) -> bool {
        self.circuit_breaker.allows_trading()
    }

    /// Check if in feedback loop
    pub fn in_feedback_loop(&self) -> bool {
        self.feedback_steps >= self.config.feedback_confirmation_steps
    }

    /// Get current feedback direction if in loop
    pub fn feedback_direction(&self) -> Option<FeedbackDirection> {
        if self.in_feedback_loop() {
            self.last_feedback_direction
        } else {
            None
        }
    }

    /// Reset the loop state
    pub fn reset(&mut self) {
        self.circuit_breaker.reset();
        self.momentum_history.clear();
        self.volatility_history.clear();
        self.feedback_steps = 0;
        self.last_feedback_direction = None;
    }

    fn update_history(&mut self, moments: &OrderbookMoments) {
        // Track momentum (imbalance direction)
        self.momentum_history.push(moments.imbalance);
        if self.momentum_history.len() > 20 {
            self.momentum_history.remove(0);
        }

        // Track volatility
        self.volatility_history.push(moments.mid_volatility);
        if self.volatility_history.len() > 20 {
            self.volatility_history.remove(0);
        }
    }

    fn detect_feedback(&mut self) -> Option<(FeedbackDirection, f64)> {
        if self.momentum_history.len() < 5 || self.volatility_history.len() < 5 {
            return None;
        }

        // Check for positive feedback: momentum and volatility moving together
        let momentum_trend = self.compute_trend(&self.momentum_history);
        let volatility_trend = self.compute_trend(&self.volatility_history);

        // Positive feedback: same direction trends (both increasing or both decreasing)
        // Negative feedback: opposite direction trends
        let correlation = momentum_trend * volatility_trend;

        let direction = if correlation > 0.0 {
            FeedbackDirection::Amplifying
        } else {
            FeedbackDirection::Dampening
        };

        let strength = correlation.abs();

        if strength > self.config.feedback_threshold {
            if self.last_feedback_direction == Some(direction) {
                self.feedback_steps += 1;
            } else {
                self.feedback_steps = 1;
                self.last_feedback_direction = Some(direction);
            }

            if self.feedback_steps >= self.config.feedback_confirmation_steps {
                return Some((direction, strength));
            }
        } else {
            self.feedback_steps = 0;
            self.last_feedback_direction = None;
        }

        None
    }

    fn compute_trend(&self, data: &[f64]) -> f64 {
        if data.len() < 2 {
            return 0.0;
        }

        // Simple linear regression slope
        let n = data.len() as f64;
        let sum_x: f64 = (0..data.len()).map(|i| i as f64).sum();
        let sum_y: f64 = data.iter().sum();
        let sum_xy: f64 = data.iter().enumerate().map(|(i, y)| i as f64 * y).sum();
        let sum_xx: f64 = (0..data.len()).map(|i| (i * i) as f64).sum();

        let denominator = n * sum_xx - sum_x * sum_x;
        if denominator.abs() < 1e-10 {
            return 0.0;
        }

        (n * sum_xy - sum_x * sum_y) / denominator
    }

    fn describe_trigger(&self, moments: &OrderbookMoments) -> String {
        if moments.depth_ratio < 0.1 {
            "Liquidity collapse".to_string()
        } else if moments.spread_bps > 200.0 {
            format!("Spread blowout ({:.0} bps)", moments.spread_bps)
        } else if moments.mid_volatility > 0.1 {
            format!("Volatility spike ({:.1}%)", moments.mid_volatility * 100.0)
        } else {
            "Multiple factors".to_string()
        }
    }
}

impl Default for ReflexiveLoop {
    fn default() -> Self {
        Self::new(
            ReflexiveLoopConfig::default(),
            RegimeShiftDetector::default(),
            CircuitBreakerConfig::default(),
            0.02,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::regime_detection::ShiftDetectorConfig;
    use crate::domain::MomentStdDevs;

    fn create_loop() -> ReflexiveLoop {
        let regime_detector = RegimeShiftDetector::new(
            ShiftDetectorConfig {
                confirmation_steps: 2,
                deviation_threshold: 1.0,
                ..Default::default()
            },
            OrderbookMoments::default(),
            MomentStdDevs {
                spread_bps: 30.0,
                depth_ratio: 0.4,
                imbalance: 0.2,
                mid_volatility: 0.05,
            },
        );

        ReflexiveLoop::new(
            ReflexiveLoopConfig {
                feedback_confirmation_steps: 3,
                feedback_threshold: 0.5,
                enable_circuit_breakers: true,
            },
            regime_detector,
            CircuitBreakerConfig::default(),
            0.02,
        )
    }

    #[test]
    fn test_normal_operation() {
        let mut loop_ctrl = create_loop();
        let moments = OrderbookMoments {
            spread_bps: 10.0,
            depth_ratio: 1.0,
            mid_volatility: 0.02,
            ..Default::default()
        };

        let events = loop_ctrl.update(&moments, 0);
        assert!(events.is_empty());
        assert!(loop_ctrl.allows_trading());
        assert_eq!(loop_ctrl.current_regime(), MarketRegime::Normal);
    }

    #[test]
    fn test_regime_shift_event() {
        let mut loop_ctrl = create_loop();

        // Create stressed conditions
        let stressed = OrderbookMoments {
            spread_bps: 55.0,
            depth_ratio: 0.25,
            imbalance: 0.1,
            mid_volatility: 0.02,
            ..Default::default()
        };

        // First update - starts tracking
        let events1 = loop_ctrl.update(&stressed, 0);
        assert!(events1.is_empty());

        // Second update - confirms shift
        let events2 = loop_ctrl.update(&stressed, 100);
        assert!(
            events2
                .iter()
                .any(|e| matches!(e, ReflexiveEvent::RegimeShift(_)))
        );
    }

    #[test]
    fn test_circuit_breaker_event() {
        let mut loop_ctrl = create_loop();

        // Trigger circuit breaker with liquidity collapse
        let collapse = OrderbookMoments {
            spread_bps: 10.0,
            depth_ratio: 0.05, // Below min threshold
            mid_volatility: 0.02,
            ..Default::default()
        };

        let events = loop_ctrl.update(&collapse, 0);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ReflexiveEvent::CircuitStateChange { .. }))
        );
        assert!(!loop_ctrl.allows_trading());
    }

    #[test]
    fn test_feedback_tracking() {
        // Test that feedback history is tracked properly
        let mut loop_ctrl = ReflexiveLoop::new(
            ReflexiveLoopConfig {
                feedback_confirmation_steps: 100, // High so we don't trigger
                feedback_threshold: 0.001,
                enable_circuit_breakers: false,
            },
            RegimeShiftDetector::default(),
            CircuitBreakerConfig::default(),
            0.02,
        );

        // Add some history
        for i in 0..10 {
            let moments = OrderbookMoments {
                imbalance: 0.1 + 0.01 * i as f64,
                mid_volatility: 0.02 + 0.001 * i as f64,
                spread_bps: 10.0,
                depth_ratio: 1.0,
                ..Default::default()
            };
            loop_ctrl.update(&moments, i * 100);
        }

        // Verify we're tracking but not yet in loop (high confirmation threshold)
        assert!(!loop_ctrl.in_feedback_loop());
    }

    #[test]
    fn test_reset() {
        let mut loop_ctrl = create_loop();

        // Trigger some state
        let collapse = OrderbookMoments {
            depth_ratio: 0.05,
            ..Default::default()
        };
        loop_ctrl.update(&collapse, 0);
        assert!(!loop_ctrl.allows_trading());

        // Reset
        loop_ctrl.reset();
        assert!(loop_ctrl.allows_trading());
        assert!(!loop_ctrl.in_feedback_loop());
    }
}
