//! Layering/Spoofing Detection
//!
//! Detects fake orders intended to create false impression of supply/demand.
//!
//! Signals:
//! - Multiple large orders at consecutive levels
//! - Same size (round lots)
//! - Placed within milliseconds
//! - Canceled before touch
//! - Opposite side activity after layering

use serde::{Deserialize, Serialize};
use trading_core::Side;

/// Spoofing alert
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpoofingAlert {
    pub confidence: super::quote_stuffing::Confidence,
    pub side: Side,
    pub num_layers: usize,
    pub cancel_ratio: f64,
    pub opposite_volume: i64,
}

/// Tracked order for spoofing detection
#[derive(Debug, Clone)]
struct TrackedOrder {
    price_level: i64,
    size: i64,
    side: Side,
    timestamp_ms: u64,
    is_canceled: bool,
}

/// Configuration for spoofing detection
#[derive(Debug, Clone, Copy)]
pub struct SpoofingConfig {
    /// Minimum number of consecutive levels to be suspicious
    pub min_layers: usize,
    /// Maximum time between layer placements (ms)
    pub layer_window_ms: u64,
    /// Cancel ratio threshold for layers
    pub layer_cancel_threshold: f64,
    /// Time window to look for opposite trades (ms)
    pub trade_window_ms: u64,
}

impl Default for SpoofingConfig {
    fn default() -> Self {
        Self {
            min_layers: 3,
            layer_window_ms: 100,
            layer_cancel_threshold: 0.9,
            trade_window_ms: 1000,
        }
    }
}

/// Spoofing detector
pub struct SpoofingDetector {
    config: SpoofingConfig,
    orders: Vec<TrackedOrder>,
    trades: Vec<(u64, Side, i64)>, // (timestamp_ms, side, volume)
}

impl SpoofingDetector {
    pub fn new(config: SpoofingConfig) -> Self {
        Self {
            config,
            orders: Vec::new(),
            trades: Vec::new(),
        }
    }

    /// Record an order placement
    pub fn record_order(&mut self, price_level: i64, size: i64, side: Side, timestamp_ms: u64) {
        self.orders.push(TrackedOrder {
            price_level,
            size,
            side,
            timestamp_ms,
            is_canceled: false,
        });
        self.cleanup_old_data(timestamp_ms);
    }

    /// Record an order cancellation
    pub fn record_cancel(&mut self, price_level: i64, side: Side) {
        for order in self.orders.iter_mut().rev() {
            if order.price_level == price_level && order.side == side && !order.is_canceled {
                order.is_canceled = true;
                break;
            }
        }
    }

    /// Record a trade
    pub fn record_trade(&mut self, side: Side, volume: i64, timestamp_ms: u64) {
        self.trades.push((timestamp_ms, side, volume));
    }

    /// Check for spoofing patterns
    pub fn check(&self, current_time_ms: u64) -> Option<SpoofingAlert> {
        // Look for layer patterns on each side
        for side in [Side::Buy, Side::Sell] {
            if let Some(alert) = self.check_side(side, current_time_ms) {
                return Some(alert);
            }
        }
        None
    }

    fn check_side(&self, side: Side, current_time_ms: u64) -> Option<SpoofingAlert> {
        // Get recent orders on this side
        let window_start = current_time_ms.saturating_sub(self.config.layer_window_ms);
        let side_orders: Vec<_> = self
            .orders
            .iter()
            .filter(|o| o.side == side && o.timestamp_ms >= window_start)
            .collect();

        if side_orders.len() < self.config.min_layers {
            return None;
        }

        // Check for consecutive price levels
        let mut price_levels: Vec<_> = side_orders.iter().map(|o| o.price_level).collect();
        price_levels.sort();
        price_levels.dedup();

        // Count consecutive levels
        let consecutive = self.count_consecutive(&price_levels);

        if consecutive < self.config.min_layers {
            return None;
        }

        // Check cancel ratio
        let canceled = side_orders.iter().filter(|o| o.is_canceled).count();
        let cancel_ratio = canceled as f64 / side_orders.len() as f64;

        if cancel_ratio < self.config.layer_cancel_threshold {
            return None;
        }

        // Check for opposite side trades
        let opposite_side = match side {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        };

        let trade_start = current_time_ms.saturating_sub(self.config.trade_window_ms);
        let opposite_volume: i64 = self
            .trades
            .iter()
            .filter(|(ts, s, _)| *ts >= trade_start && *s == opposite_side)
            .map(|(_, _, v)| v)
            .sum();

        let confidence = if cancel_ratio > 0.95 && consecutive >= 4 {
            super::quote_stuffing::Confidence::High
        } else if cancel_ratio > 0.9 {
            super::quote_stuffing::Confidence::Medium
        } else {
            super::quote_stuffing::Confidence::Low
        };

        Some(SpoofingAlert {
            confidence,
            side,
            num_layers: consecutive,
            cancel_ratio,
            opposite_volume,
        })
    }

    fn count_consecutive(&self, sorted_levels: &[i64]) -> usize {
        if sorted_levels.is_empty() {
            return 0;
        }

        let tick_size = self.estimate_tick_size(sorted_levels);
        let mut max_consecutive = 1;
        let mut current_consecutive = 1;

        for i in 1..sorted_levels.len() {
            if sorted_levels[i] - sorted_levels[i - 1] <= tick_size * 2 {
                current_consecutive += 1;
                max_consecutive = max_consecutive.max(current_consecutive);
            } else {
                current_consecutive = 1;
            }
        }

        max_consecutive
    }

    fn estimate_tick_size(&self, sorted_levels: &[i64]) -> i64 {
        if sorted_levels.len() < 2 {
            return 1_00000000; // Default 1 tick
        }

        // Find minimum difference
        let mut min_diff = i64::MAX;
        for i in 1..sorted_levels.len() {
            let diff = sorted_levels[i] - sorted_levels[i - 1];
            if diff > 0 && diff < min_diff {
                min_diff = diff;
            }
        }

        if min_diff == i64::MAX {
            1_00000000
        } else {
            min_diff
        }
    }

    fn cleanup_old_data(&mut self, current_time_ms: u64) {
        let cutoff = current_time_ms.saturating_sub(10000); // Keep 10 seconds
        self.orders.retain(|o| o.timestamp_ms >= cutoff);
        self.trades.retain(|(ts, _, _)| *ts >= cutoff);
    }
}

impl Default for SpoofingDetector {
    fn default() -> Self {
        Self::new(SpoofingConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_alert_normal_orders() {
        let mut detector = SpoofingDetector::default();

        detector.record_order(100_00000000, 10, Side::Buy, 0);
        detector.record_order(99_00000000, 10, Side::Buy, 100);

        let alert = detector.check(200);
        assert!(alert.is_none());
    }

    #[test]
    fn test_alert_layering_pattern() {
        let mut detector = SpoofingDetector::new(SpoofingConfig {
            min_layers: 3,
            layer_window_ms: 100,
            layer_cancel_threshold: 0.9,
            ..Default::default()
        });

        // Create layering pattern
        for i in 0..5 {
            let price = (100 - i) * 100000000;
            detector.record_order(price, 1000, Side::Buy, i as u64 * 10);
        }

        // Cancel all
        for i in 0..5 {
            let price = (100 - i) * 100000000;
            detector.record_cancel(price, Side::Buy);
        }

        let alert = detector.check(100);
        assert!(alert.is_some());
    }
}
