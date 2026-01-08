//! Momentum Ignition Detection
//!
//! Detects triggering of momentum algorithms to ride artificial trends.
//!
//! Signals:
//! - Aggressive initial trades from concentrated aggressor
//! - Unusual volume concentration
//! - Quick reversal pattern
//! - Spread behavior during move

use serde::{Deserialize, Serialize};
use trading_core::Side;

/// Momentum ignition alert
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MomentumIgnitionAlert {
    pub confidence: super::quote_stuffing::Confidence,
    pub initiator_concentration: f64,
    pub momentum_bps: f64,
    pub reversal_detected: bool,
}

/// Trade record for analysis
#[derive(Debug, Clone)]
struct TradeRecord {
    participant_id: u64,
    side: Side,
    size: i64,
    price: i64,
    timestamp_ms: u64,
}

/// Configuration for momentum ignition detection
#[derive(Debug, Clone, Copy)]
pub struct MomentumIgnitionConfig {
    /// Threshold for single participant volume concentration
    pub concentration_threshold: f64,
    /// Price move threshold for momentum (bps)
    pub momentum_threshold_bps: f64,
    /// Time window for analysis (ms)
    pub analysis_window_ms: u64,
    /// Time for reversal detection (ms)
    pub reversal_window_ms: u64,
}

impl Default for MomentumIgnitionConfig {
    fn default() -> Self {
        Self {
            concentration_threshold: 0.4,
            momentum_threshold_bps: 20.0,
            analysis_window_ms: 5000,
            reversal_window_ms: 2500,
        }
    }
}

/// Momentum ignition detector
pub struct MomentumIgnitionDetector {
    config: MomentumIgnitionConfig,
    trades: Vec<TradeRecord>,
    initial_mid_price: Option<i64>,
}

impl MomentumIgnitionDetector {
    pub fn new(config: MomentumIgnitionConfig) -> Self {
        Self {
            config,
            trades: Vec::new(),
            initial_mid_price: None,
        }
    }

    /// Set reference price at start of analysis window
    pub fn set_reference_price(&mut self, mid_price: i64) {
        if self.initial_mid_price.is_none() {
            self.initial_mid_price = Some(mid_price);
        }
    }

    /// Record a trade
    pub fn record_trade(
        &mut self,
        participant_id: u64,
        side: Side,
        size: i64,
        price: i64,
        timestamp_ms: u64,
    ) {
        self.trades.push(TradeRecord {
            participant_id,
            side,
            size,
            price,
            timestamp_ms,
        });
        self.cleanup_old_data(timestamp_ms);
    }

    /// Check for momentum ignition
    pub fn check(
        &self,
        current_time_ms: u64,
        current_mid_price: i64,
    ) -> Option<MomentumIgnitionAlert> {
        if self.trades.len() < 10 {
            return None;
        }

        let initial_price = self.initial_mid_price?;

        // Calculate price momentum
        let momentum_bps =
            ((current_mid_price - initial_price) as f64 / initial_price as f64) * 10_000.0;

        if momentum_bps.abs() < self.config.momentum_threshold_bps {
            return None;
        }

        // Find concentrated aggressor
        let (initiator_concentration, dominant_side) =
            self.find_concentrated_aggressor(current_time_ms)?;

        if initiator_concentration < self.config.concentration_threshold {
            return None;
        }

        // Check for reversal
        let reversal_detected = self.detect_reversal(current_time_ms, dominant_side);

        let confidence = if reversal_detected && initiator_concentration > 0.6 {
            super::quote_stuffing::Confidence::High
        } else if reversal_detected || initiator_concentration > 0.5 {
            super::quote_stuffing::Confidence::Medium
        } else {
            super::quote_stuffing::Confidence::Low
        };

        Some(MomentumIgnitionAlert {
            confidence,
            initiator_concentration,
            momentum_bps,
            reversal_detected,
        })
    }

    fn find_concentrated_aggressor(&self, current_time_ms: u64) -> Option<(f64, Side)> {
        use std::collections::HashMap;

        let window_start = current_time_ms.saturating_sub(self.config.analysis_window_ms / 2);
        let early_trades: Vec<_> = self
            .trades
            .iter()
            .filter(|t| t.timestamp_ms <= window_start)
            .collect();

        if early_trades.is_empty() {
            return None;
        }

        // Count volume by participant
        let mut volume_by_participant: HashMap<u64, i64> = HashMap::new();
        let mut total_volume = 0i64;
        let mut buy_volume = 0i64;

        for trade in &early_trades {
            *volume_by_participant
                .entry(trade.participant_id)
                .or_default() += trade.size;
            total_volume += trade.size;
            if trade.side == Side::Buy {
                buy_volume += trade.size;
            }
        }

        if total_volume == 0 {
            return None;
        }

        // Find maximum concentration
        let max_concentration = volume_by_participant
            .values()
            .map(|v| *v as f64 / total_volume as f64)
            .fold(0.0, f64::max);

        let dominant_side = if buy_volume > total_volume / 2 {
            Side::Buy
        } else {
            Side::Sell
        };

        Some((max_concentration, dominant_side))
    }

    fn detect_reversal(&self, current_time_ms: u64, initial_side: Side) -> bool {
        let reversal_start = current_time_ms.saturating_sub(self.config.reversal_window_ms);
        let late_trades: Vec<_> = self
            .trades
            .iter()
            .filter(|t| t.timestamp_ms >= reversal_start)
            .collect();

        if late_trades.is_empty() {
            return false;
        }

        // Count reversal trades (opposite side)
        let opposite_volume: i64 = late_trades
            .iter()
            .filter(|t| t.side != initial_side)
            .map(|t| t.size)
            .sum();

        let total_late_volume: i64 = late_trades.iter().map(|t| t.size).sum();

        if total_late_volume == 0 {
            return false;
        }

        // Reversal if opposite side > 50%
        opposite_volume > total_late_volume / 2
    }

    fn cleanup_old_data(&mut self, current_time_ms: u64) {
        let cutoff = current_time_ms.saturating_sub(self.config.analysis_window_ms * 2);
        self.trades.retain(|t| t.timestamp_ms >= cutoff);
    }

    /// Reset the detector
    pub fn reset(&mut self) {
        self.trades.clear();
        self.initial_mid_price = None;
    }
}

impl Default for MomentumIgnitionDetector {
    fn default() -> Self {
        Self::new(MomentumIgnitionConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_alert_normal_trading() {
        let mut detector = MomentumIgnitionDetector::default();

        detector.set_reference_price(100_00000000);

        // Mixed trading from different participants
        for i in 0..20 {
            let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
            detector.record_trade(i % 5, side, 10, 100_00000000, i * 100);
        }

        let alert = detector.check(2000, 100_10000000);
        // Not concentrated enough and small move
        assert!(alert.is_none());
    }

    #[test]
    fn test_alert_concentrated_momentum() {
        let mut detector = MomentumIgnitionDetector::new(MomentumIgnitionConfig {
            concentration_threshold: 0.4,
            momentum_threshold_bps: 10.0,
            analysis_window_ms: 1000,
            reversal_window_ms: 500,
        });

        detector.set_reference_price(100_00000000);

        // Single participant dominates early
        for i in 0..10 {
            detector.record_trade(
                1,
                Side::Buy,
                100,
                (100_00000000 + i * 1000000) as i64,
                i * 50,
            );
        }

        // Later reversal by same participant
        for i in 0..5 {
            detector.record_trade(1, Side::Sell, 80, 101_00000000, 600 + i * 50);
        }

        let alert = detector.check(850, 101_00000000);
        // Should detect: high concentration, momentum, reversal
        assert!(alert.is_some());
    }
}
