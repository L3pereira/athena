//! Quote Stuffing Detection
//!
//! Detects flooding of market with orders to slow competitor feeds.
//!
//! Signals:
//! - Message rate anomaly: 10-100x normal
//! - Cancel ratio spike: 95%+ cancel rate
//! - Burst pattern: Orders in <100ms bursts

use serde::{Deserialize, Serialize};

/// Quote stuffing alert
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteStuffingAlert {
    pub confidence: Confidence,
    pub message_rate: f64,
    pub cancel_ratio: f64,
    pub burst_duration_ms: u64,
}

/// Confidence level for alerts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// Configuration for quote stuffing detection
#[derive(Debug, Clone, Copy)]
pub struct QuoteStuffingConfig {
    /// Normal message rate (messages per second)
    pub baseline_rate: f64,
    /// Threshold multiplier for anomaly detection
    pub rate_threshold_multiplier: f64,
    /// Cancel ratio threshold
    pub cancel_ratio_threshold: f64,
    /// Window for burst detection (milliseconds)
    pub burst_window_ms: u64,
}

impl Default for QuoteStuffingConfig {
    fn default() -> Self {
        Self {
            baseline_rate: 1000.0,
            rate_threshold_multiplier: 10.0,
            cancel_ratio_threshold: 0.95,
            burst_window_ms: 100,
        }
    }
}

/// Quote stuffing detector
pub struct QuoteStuffingDetector {
    config: QuoteStuffingConfig,
    message_count: u64,
    cancel_count: u64,
    window_start_ms: u64,
    current_window_messages: u64,
}

impl QuoteStuffingDetector {
    pub fn new(config: QuoteStuffingConfig) -> Self {
        Self {
            config,
            message_count: 0,
            cancel_count: 0,
            window_start_ms: 0,
            current_window_messages: 0,
        }
    }

    /// Record an order message
    pub fn record_message(&mut self, timestamp_ms: u64, is_cancel: bool) {
        self.message_count += 1;
        if is_cancel {
            self.cancel_count += 1;
        }

        // Reset window if needed
        if timestamp_ms - self.window_start_ms > self.config.burst_window_ms {
            self.window_start_ms = timestamp_ms;
            self.current_window_messages = 1;
        } else {
            self.current_window_messages += 1;
        }
    }

    /// Check for quote stuffing (call at regular intervals)
    pub fn check(&self, elapsed_secs: f64) -> Option<QuoteStuffingAlert> {
        if elapsed_secs <= 0.0 {
            return None;
        }

        let message_rate = self.message_count as f64 / elapsed_secs;
        let cancel_ratio = if self.message_count > 0 {
            self.cancel_count as f64 / self.message_count as f64
        } else {
            0.0
        };

        // Calculate burst rate
        let burst_rate =
            self.current_window_messages as f64 * (1000.0 / self.config.burst_window_ms as f64);

        let rate_exceeded =
            message_rate > self.config.baseline_rate * self.config.rate_threshold_multiplier;
        let cancel_exceeded = cancel_ratio > self.config.cancel_ratio_threshold;
        let burst_detected =
            burst_rate > self.config.baseline_rate * self.config.rate_threshold_multiplier;

        if rate_exceeded && cancel_exceeded {
            Some(QuoteStuffingAlert {
                confidence: Confidence::High,
                message_rate,
                cancel_ratio,
                burst_duration_ms: self.config.burst_window_ms,
            })
        } else if (rate_exceeded || burst_detected) && cancel_ratio > 0.8 {
            Some(QuoteStuffingAlert {
                confidence: Confidence::Medium,
                message_rate,
                cancel_ratio,
                burst_duration_ms: self.config.burst_window_ms,
            })
        } else if burst_detected && cancel_ratio > 0.7 {
            Some(QuoteStuffingAlert {
                confidence: Confidence::Low,
                message_rate,
                cancel_ratio,
                burst_duration_ms: self.config.burst_window_ms,
            })
        } else {
            None
        }
    }

    /// Reset counters
    pub fn reset(&mut self) {
        self.message_count = 0;
        self.cancel_count = 0;
        self.current_window_messages = 0;
    }
}

impl Default for QuoteStuffingDetector {
    fn default() -> Self {
        Self::new(QuoteStuffingConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_alert_normal_activity() {
        let mut detector = QuoteStuffingDetector::new(QuoteStuffingConfig {
            baseline_rate: 1000.0,
            rate_threshold_multiplier: 10.0,
            ..Default::default()
        });

        for i in 0..1000 {
            detector.record_message(i * 10, false);
        }

        let alert = detector.check(10.0); // 100 msg/sec
        assert!(alert.is_none());
    }

    #[test]
    fn test_high_alert_stuffing() {
        let mut detector = QuoteStuffingDetector::new(QuoteStuffingConfig {
            baseline_rate: 100.0,
            rate_threshold_multiplier: 10.0,
            cancel_ratio_threshold: 0.95,
            ..Default::default()
        });

        // Massive burst with high cancel rate
        for i in 0..10000 {
            detector.record_message(i, true); // All cancels
        }

        let alert = detector.check(1.0); // 10000 msg/sec
        assert!(alert.is_some());
        assert_eq!(alert.unwrap().confidence, Confidence::High);
    }
}
