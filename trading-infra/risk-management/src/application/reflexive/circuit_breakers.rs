//! Circuit Breakers
//!
//! Halt trading when reflexive feedback becomes dangerous.
//!
//! Triggers:
//! - Extreme volatility spike
//! - Liquidity collapse
//! - Rapid regime deterioration

use crate::domain::{MarketRegime, OrderbookMoments, RegimeShift};
use serde::{Deserialize, Serialize};

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation
    Normal,
    /// Warning level - increased monitoring
    Warning,
    /// Halt new orders, allow cancellations
    HaltNewOrders,
    /// Full halt - no trading activity
    FullHalt,
}

impl CircuitState {
    /// Check if trading is allowed
    pub fn allows_new_orders(&self) -> bool {
        matches!(self, CircuitState::Normal | CircuitState::Warning)
    }

    /// Check if cancellations are allowed
    pub fn allows_cancellations(&self) -> bool {
        !matches!(self, CircuitState::FullHalt)
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerConfig {
    /// Volatility threshold for warning (multiplier over baseline)
    pub volatility_warning_mult: f64,
    /// Volatility threshold for halt
    pub volatility_halt_mult: f64,
    /// Minimum depth ratio before halt
    pub min_depth_ratio: f64,
    /// Spread threshold for warning (bps)
    pub spread_warning_bps: f64,
    /// Spread threshold for halt (bps)
    pub spread_halt_bps: f64,
    /// Number of consecutive crisis detections for halt
    pub crisis_count_threshold: u32,
    /// Cooldown period after halt (ms)
    pub cooldown_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            volatility_warning_mult: 3.0,
            volatility_halt_mult: 5.0,
            min_depth_ratio: 0.1,
            spread_warning_bps: 100.0,
            spread_halt_bps: 200.0,
            crisis_count_threshold: 3,
            cooldown_ms: 30_000, // 30 seconds
        }
    }
}

/// Circuit breaker for reflexive loop protection
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    baseline_volatility: f64,
    current_state: CircuitState,
    crisis_count: u32,
    halt_timestamp_ms: Option<u64>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig, baseline_volatility: f64) -> Self {
        Self {
            config,
            baseline_volatility: baseline_volatility.max(0.01),
            current_state: CircuitState::Normal,
            crisis_count: 0,
            halt_timestamp_ms: None,
        }
    }

    /// Update circuit breaker state based on current moments
    pub fn update(&mut self, moments: &OrderbookMoments, current_time_ms: u64) -> CircuitState {
        // Check cooldown
        if let Some(halt_time) = self.halt_timestamp_ms {
            if current_time_ms < halt_time + self.config.cooldown_ms {
                return self.current_state;
            }
        }

        let vol_ratio = moments.mid_volatility / self.baseline_volatility;

        // Full halt conditions
        if moments.depth_ratio < self.config.min_depth_ratio
            || moments.spread_bps > self.config.spread_halt_bps
            || vol_ratio > self.config.volatility_halt_mult
        {
            self.trigger_halt(CircuitState::FullHalt, current_time_ms);
            return self.current_state;
        }

        // Halt new orders conditions
        if self.crisis_count >= self.config.crisis_count_threshold {
            self.trigger_halt(CircuitState::HaltNewOrders, current_time_ms);
            return self.current_state;
        }

        // Warning conditions
        if moments.spread_bps > self.config.spread_warning_bps
            || vol_ratio > self.config.volatility_warning_mult
        {
            self.current_state = CircuitState::Warning;
            return self.current_state;
        }

        // All clear
        self.current_state = CircuitState::Normal;
        self.crisis_count = 0;
        self.halt_timestamp_ms = None;
        self.current_state
    }

    /// Process a regime shift
    pub fn on_regime_shift(&mut self, shift: &RegimeShift, current_time_ms: u64) {
        if shift.to == MarketRegime::Crisis {
            self.crisis_count += 1;
            if self.crisis_count >= self.config.crisis_count_threshold {
                self.trigger_halt(CircuitState::HaltNewOrders, current_time_ms);
            }
        } else if shift.to == MarketRegime::Normal {
            self.crisis_count = 0;
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        self.current_state
    }

    /// Check if trading is allowed
    pub fn allows_trading(&self) -> bool {
        self.current_state.allows_new_orders()
    }

    /// Manually reset the circuit breaker
    pub fn reset(&mut self) {
        self.current_state = CircuitState::Normal;
        self.crisis_count = 0;
        self.halt_timestamp_ms = None;
    }

    /// Update baseline volatility
    pub fn update_baseline(&mut self, new_baseline: f64) {
        self.baseline_volatility = new_baseline.max(0.01);
    }

    fn trigger_halt(&mut self, state: CircuitState, current_time_ms: u64) {
        if self.halt_timestamp_ms.is_none() {
            self.halt_timestamp_ms = Some(current_time_ms);
        }
        self.current_state = state;
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(CircuitBreakerConfig::default(), 0.02)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_conditions() {
        let mut breaker = CircuitBreaker::default();
        let moments = OrderbookMoments {
            spread_bps: 10.0,
            depth_ratio: 1.0,
            mid_volatility: 0.02,
            ..Default::default()
        };

        let state = breaker.update(&moments, 0);
        assert_eq!(state, CircuitState::Normal);
        assert!(breaker.allows_trading());
    }

    #[test]
    fn test_warning_on_high_spread() {
        let mut breaker = CircuitBreaker::default();
        let moments = OrderbookMoments {
            spread_bps: 120.0, // Above warning threshold
            depth_ratio: 1.0,
            mid_volatility: 0.02,
            ..Default::default()
        };

        let state = breaker.update(&moments, 0);
        assert_eq!(state, CircuitState::Warning);
        assert!(breaker.allows_trading()); // Still allows trading
    }

    #[test]
    fn test_halt_on_liquidity_collapse() {
        let mut breaker = CircuitBreaker::default();
        let moments = OrderbookMoments {
            spread_bps: 10.0,
            depth_ratio: 0.05, // Below min threshold
            mid_volatility: 0.02,
            ..Default::default()
        };

        let state = breaker.update(&moments, 0);
        assert_eq!(state, CircuitState::FullHalt);
        assert!(!breaker.allows_trading());
    }

    #[test]
    fn test_halt_after_crisis_count() {
        let mut breaker = CircuitBreaker::new(
            CircuitBreakerConfig {
                crisis_count_threshold: 2,
                ..Default::default()
            },
            0.02,
        );

        // First crisis
        let shift1 = RegimeShift::new(MarketRegime::Normal, MarketRegime::Crisis, 0.9, "test");
        breaker.on_regime_shift(&shift1, 0);
        assert!(breaker.allows_trading());

        // Second crisis - should halt
        let shift2 = RegimeShift::new(MarketRegime::Stressed, MarketRegime::Crisis, 0.95, "test");
        breaker.on_regime_shift(&shift2, 100);
        assert!(!breaker.allows_trading());
    }

    #[test]
    fn test_cooldown() {
        let mut breaker = CircuitBreaker::new(
            CircuitBreakerConfig {
                cooldown_ms: 1000,
                ..Default::default()
            },
            0.02,
        );

        // Trigger halt
        let bad_moments = OrderbookMoments {
            depth_ratio: 0.05,
            ..Default::default()
        };
        breaker.update(&bad_moments, 0);
        assert_eq!(breaker.state(), CircuitState::FullHalt);

        // During cooldown, good conditions don't reset
        let good_moments = OrderbookMoments {
            spread_bps: 10.0,
            depth_ratio: 1.0,
            mid_volatility: 0.02,
            ..Default::default()
        };
        breaker.update(&good_moments, 500);
        assert_eq!(breaker.state(), CircuitState::FullHalt);

        // After cooldown, can return to normal
        breaker.update(&good_moments, 1500);
        assert_eq!(breaker.state(), CircuitState::Normal);
    }
}
