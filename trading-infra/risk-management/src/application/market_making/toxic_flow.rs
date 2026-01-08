//! Toxic Flow Detection
//!
//! VPIN and order flow imbalance detection.
//!
//! From docs Section 8:
//! VPIN = |Buy Volume − Sell Volume| / Total Volume
//!
//! Interpretation:
//! ~0.3  → Normal, balanced flow
//! ~0.5  → Elevated, some informed activity
//! ~0.7+ → High toxicity, spreads should widen
//! ~0.9+ → Extreme, consider pulling quotes

use crate::domain::{ToxicityLevel, ToxicityMetrics};

/// Configuration for toxic flow detection
#[derive(Debug, Clone, Copy)]
pub struct ToxicFlowConfig {
    /// VPIN bucket size (volume units)
    pub bucket_size: i64,
    /// Number of buckets for VPIN calculation
    pub num_buckets: usize,
    /// Threshold for elevated toxicity
    pub elevated_threshold: f64,
    /// Threshold for high toxicity
    pub high_threshold: f64,
    /// Threshold for extreme toxicity
    pub extreme_threshold: f64,
}

impl Default for ToxicFlowConfig {
    fn default() -> Self {
        Self {
            bucket_size: 1000_00000000, // 1000 units per bucket
            num_buckets: 50,
            elevated_threshold: 0.45,
            high_threshold: 0.65,
            extreme_threshold: 0.85,
        }
    }
}

/// Volume bucket for VPIN calculation
#[derive(Debug, Clone, Copy, Default)]
struct VolumeBucket {
    buy_volume: i64,
    sell_volume: i64,
}

impl VolumeBucket {
    fn total(&self) -> i64 {
        self.buy_volume + self.sell_volume
    }

    fn imbalance(&self) -> i64 {
        (self.buy_volume - self.sell_volume).abs()
    }
}

/// Toxic flow detector using VPIN methodology
pub struct ToxicFlowDetector {
    config: ToxicFlowConfig,
    buckets: Vec<VolumeBucket>,
    current_bucket: VolumeBucket,
    bucket_index: usize,
    /// Order flow imbalance tracking
    bid_improvement: f64,
    bid_deterioration: f64,
    ask_improvement: f64,
    ask_deterioration: f64,
}

impl ToxicFlowDetector {
    pub fn new(config: ToxicFlowConfig) -> Self {
        Self {
            config,
            buckets: vec![VolumeBucket::default(); config.num_buckets],
            current_bucket: VolumeBucket::default(),
            bucket_index: 0,
            bid_improvement: 0.0,
            bid_deterioration: 0.0,
            ask_improvement: 0.0,
            ask_deterioration: 0.0,
        }
    }

    /// Record a trade
    pub fn record_trade(&mut self, volume: i64, is_buy: bool) {
        if is_buy {
            self.current_bucket.buy_volume += volume;
        } else {
            self.current_bucket.sell_volume += volume;
        }

        // Check if bucket is full
        if self.current_bucket.total() >= self.config.bucket_size {
            // Store bucket and move to next
            self.buckets[self.bucket_index] = self.current_bucket;
            self.bucket_index = (self.bucket_index + 1) % self.config.num_buckets;
            self.current_bucket = VolumeBucket::default();
        }
    }

    /// Record order book change for OFI
    pub fn record_book_change(
        &mut self,
        bid_improved: bool,
        bid_deteriorated: bool,
        ask_improved: bool,
        ask_deteriorated: bool,
    ) {
        if bid_improved {
            self.bid_improvement += 1.0;
        }
        if bid_deteriorated {
            self.bid_deterioration += 1.0;
        }
        if ask_improved {
            self.ask_improvement += 1.0;
        }
        if ask_deteriorated {
            self.ask_deterioration += 1.0;
        }
    }

    /// Calculate VPIN
    fn calculate_vpin(&self) -> f64 {
        let total_imbalance: i64 = self.buckets.iter().map(|b| b.imbalance()).sum();
        let total_volume: i64 = self.buckets.iter().map(|b| b.total()).sum();

        if total_volume == 0 {
            return 0.3; // Default to normal
        }

        total_imbalance as f64 / total_volume as f64
    }

    /// Calculate OFI
    fn calculate_ofi(&self) -> f64 {
        let bid_net = self.bid_improvement - self.bid_deterioration;
        let ask_net = self.ask_improvement - self.ask_deterioration;
        let total = self.bid_improvement
            + self.bid_deterioration
            + self.ask_improvement
            + self.ask_deterioration;

        if total == 0.0 {
            return 0.0;
        }

        (bid_net - ask_net) / total
    }

    /// Get current toxicity metrics
    pub fn metrics(&self) -> ToxicityMetrics {
        let vpin = self.calculate_vpin();
        let ofi = self.calculate_ofi();
        ToxicityMetrics::new(vpin, ofi)
    }

    /// Get current toxicity level
    pub fn level(&self) -> ToxicityLevel {
        self.metrics().level
    }

    /// Check if we should widen spreads
    pub fn should_widen(&self) -> bool {
        let level = self.level();
        matches!(
            level,
            ToxicityLevel::Elevated | ToxicityLevel::High | ToxicityLevel::Extreme
        )
    }

    /// Check if we should pull quotes
    pub fn should_pull(&self) -> bool {
        matches!(self.level(), ToxicityLevel::Extreme)
    }

    /// Reset OFI counters (call periodically)
    pub fn reset_ofi(&mut self) {
        self.bid_improvement = 0.0;
        self.bid_deterioration = 0.0;
        self.ask_improvement = 0.0;
        self.ask_deterioration = 0.0;
    }

    /// Get spread multiplier based on current toxicity
    pub fn spread_multiplier(&self) -> f64 {
        self.level().spread_multiplier()
    }

    /// Get depth multiplier based on current toxicity
    pub fn depth_multiplier(&self) -> f64 {
        self.level().depth_multiplier()
    }
}

impl Default for ToxicFlowDetector {
    fn default() -> Self {
        Self::new(ToxicFlowConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balanced_flow_normal() {
        let mut detector = ToxicFlowDetector::new(ToxicFlowConfig {
            bucket_size: 100,
            num_buckets: 10,
            ..Default::default()
        });

        // Balanced trades
        for _ in 0..20 {
            detector.record_trade(50, true);
            detector.record_trade(50, false);
        }

        let metrics = detector.metrics();
        assert_eq!(metrics.level, ToxicityLevel::Normal);
    }

    #[test]
    fn test_imbalanced_flow_elevated() {
        let mut detector = ToxicFlowDetector::new(ToxicFlowConfig {
            bucket_size: 100,
            num_buckets: 10,
            ..Default::default()
        });

        // Imbalanced trades (70% buy)
        for _ in 0..50 {
            detector.record_trade(70, true);
            detector.record_trade(30, false);
        }

        let metrics = detector.metrics();
        assert!(metrics.vpin > 0.3);
    }

    #[test]
    fn test_extreme_imbalance() {
        let mut detector = ToxicFlowDetector::new(ToxicFlowConfig {
            bucket_size: 100,
            num_buckets: 10,
            ..Default::default()
        });

        // Extreme imbalance (all buys)
        for _ in 0..50 {
            detector.record_trade(100, true);
        }

        assert!(detector.should_pull());
    }

    #[test]
    fn test_ofi_calculation() {
        let mut detector = ToxicFlowDetector::default();

        // Strong bid improvement
        for _ in 0..10 {
            detector.record_book_change(true, false, false, false);
        }

        let ofi = detector.calculate_ofi();
        assert!(ofi > 0.5); // Positive = bid pressure
    }

    #[test]
    fn test_spread_multiplier() {
        let detector = ToxicFlowDetector::default();
        // Default should be normal
        assert!((detector.spread_multiplier() - 1.0).abs() < 0.01);
    }
}
