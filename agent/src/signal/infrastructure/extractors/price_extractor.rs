//! Price Feature Extractor - Infrastructure implementation
//!
//! Implements StatefulFeatureExtractor for price-based rolling statistics.
//! Uses trading-core's integer-based statistics for precision.

use crate::signal::application::ports::StatefulFeatureExtractor;
use crate::signal::domain::Features;
use trading_core::{Ema, PRICE_SCALE, RollingStats};

/// Price-based feature extractor with rolling statistics
///
/// Maintains internal state for:
/// - Rolling price window
/// - Rolling return window
/// - EMAs (fast and slow)
#[derive(Debug, Clone)]
pub struct PriceExtractor {
    /// Window size for rolling calculations
    #[allow(dead_code)]
    window_size: usize,
    /// Rolling statistics for prices
    price_stats: RollingStats,
    /// Rolling statistics for returns
    return_stats: RollingStats,
    /// Fast EMA
    ema_fast: Ema,
    /// Slow EMA
    ema_slow: Ema,
    /// Last price (scaled)
    last_price: Option<i64>,
}

impl PriceExtractor {
    /// Create with default parameters (window=20, fast=5, slow=20)
    pub fn new() -> Self {
        Self::with_params(20, 5, 20)
    }

    /// Create with custom parameters
    pub fn with_params(window_size: usize, ema_fast_period: usize, ema_slow_period: usize) -> Self {
        Self {
            window_size: window_size.max(2),
            price_stats: RollingStats::new(window_size.max(2), 8),
            return_stats: RollingStats::new(window_size.max(2), 8),
            ema_fast: Ema::from_period(ema_fast_period.max(2), 8),
            ema_slow: Ema::from_period(ema_slow_period.max(2), 8),
            last_price: None,
        }
    }
}

impl Default for PriceExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulFeatureExtractor for PriceExtractor {
    fn update(&mut self, mid_price: f64) -> Features {
        let mut features = Features::new();

        // Convert to scaled integer
        let price_raw = (mid_price * PRICE_SCALE as f64) as i64;

        // Calculate return if we have history
        if let Some(last_raw) = self.last_price
            && last_raw != 0
        {
            // Simple return: (new - old) / old
            let simple_return = (mid_price - last_raw as f64 / PRICE_SCALE as f64)
                / (last_raw as f64 / PRICE_SCALE as f64);

            // Log return
            let log_return = (mid_price / (last_raw as f64 / PRICE_SCALE as f64)).ln();

            features.set("price_return", simple_return);
            features.set("log_return", log_return);

            // Store return (scaled by PRICE_SCALE for integer stats)
            let return_scaled = (log_return * PRICE_SCALE as f64) as i64;
            self.return_stats.push(return_scaled);
        }

        // Store price
        self.price_stats.push(price_raw);
        self.last_price = Some(price_raw);

        // Update EMAs
        self.ema_fast.update(price_raw);
        self.ema_slow.update(price_raw);

        // Calculate volatility from returns
        if let Some(std_dev) = self.return_stats.std_dev() {
            let volatility = std_dev as f64 / PRICE_SCALE as f64;
            if volatility.is_finite() {
                features.set("volatility", volatility);
            }
        }

        // Mean return
        if let Some(mean) = self.return_stats.mean() {
            let mean_return = mean as f64 / PRICE_SCALE as f64;
            if mean_return.is_finite() {
                features.set("mean_return", mean_return);
            }
        }

        // Price statistics for z-score
        if let Some(z) = self.price_stats.z_score(price_raw) {
            let z_score = z as f64 / PRICE_SCALE as f64;
            if z_score.is_finite() {
                features.set("z_score", z_score);
            }
        }

        // EMA features
        if let (Some(fast), Some(slow)) = (self.ema_fast.value_i64(), self.ema_slow.value_i64()) {
            let fast_f64 = fast as f64 / PRICE_SCALE as f64;
            let slow_f64 = slow as f64 / PRICE_SCALE as f64;

            features.set("ema_fast", fast_f64);
            features.set("ema_slow", slow_f64);

            // EMA signal: positive when fast > slow (bullish)
            let ema_signal = fast_f64 - slow_f64;
            features.set("ema_signal", ema_signal);

            // Normalized signal
            if slow_f64.abs() > 1e-10 {
                let ema_signal_pct = ema_signal / slow_f64;
                features.set("ema_signal_pct", ema_signal_pct);
            }
        }

        // Momentum: change from first to last in window
        if let Some(momentum) = self.price_stats.momentum() {
            let mom = momentum as f64 / PRICE_SCALE as f64;
            if mom.is_finite() {
                features.set("momentum", mom);
            }
        }

        features
    }

    fn reset(&mut self) {
        self.price_stats.clear();
        self.return_stats.clear();
        self.ema_fast.reset();
        self.ema_slow.reset();
        self.last_price = None;
    }

    fn feature_names(&self) -> Vec<&'static str> {
        vec![
            "price_return",
            "log_return",
            "volatility",
            "mean_return",
            "z_score",
            "ema_fast",
            "ema_slow",
            "ema_signal",
            "ema_signal_pct",
            "momentum",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_extractor_basic() {
        let mut extractor = PriceExtractor::new();

        // First update - no return yet
        let features = extractor.update(100.0);
        assert!(features.get("price_return").is_none());
        assert!(features.get("ema_fast").is_some());

        // Second update - should have return
        let features = extractor.update(101.0);
        let ret = features.get("price_return").unwrap();
        assert!((ret - 0.01).abs() < 0.001); // 1% return
    }

    #[test]
    fn test_ema_convergence() {
        let mut extractor = PriceExtractor::with_params(10, 3, 10);

        // Feed constant prices - EMAs should converge
        for _ in 0..20 {
            extractor.update(100.0);
        }

        let features = extractor.update(100.0);
        let ema_fast = features.get("ema_fast").unwrap();
        let ema_slow = features.get("ema_slow").unwrap();

        assert!((ema_fast - 100.0).abs() < 0.1);
        assert!((ema_slow - 100.0).abs() < 0.1);

        // Now feed higher prices - fast should react quicker
        for _ in 0..5 {
            extractor.update(110.0);
        }

        let features = extractor.update(110.0);
        let ema_fast = features.get("ema_fast").unwrap();
        let ema_slow = features.get("ema_slow").unwrap();
        assert!(ema_fast > ema_slow);
    }

    #[test]
    fn test_volatility_calculation() {
        let mut extractor = PriceExtractor::with_params(10, 5, 10);

        // Feed constant prices - volatility should be low
        for _ in 0..10 {
            extractor.update(100.0);
        }
        let features = extractor.update(100.0);
        if let Some(vol) = features.get("volatility") {
            assert!(
                vol.abs() < 0.001,
                "constant prices should have near-zero volatility"
            );
        }

        // Feed varying prices - volatility should increase
        let mut extractor2 = PriceExtractor::with_params(10, 5, 10);
        for i in 0..10 {
            extractor2.update(100.0 + (i as f64 * 2.0) * if i % 2 == 0 { 1.0 } else { -1.0 });
        }
        let features2 = extractor2.update(100.0);
        if let Some(vol) = features2.get("volatility") {
            assert!(vol > 0.01, "varying prices should have positive volatility");
        }
    }

    #[test]
    fn test_reset() {
        let mut extractor = PriceExtractor::new();

        // Add some data
        for i in 0..10 {
            extractor.update(100.0 + i as f64);
        }

        assert!(extractor.ema_fast.is_initialized());
        assert!(!extractor.price_stats.is_empty());

        // Reset
        extractor.reset();

        assert!(!extractor.ema_fast.is_initialized());
        assert!(!extractor.ema_slow.is_initialized());
        assert!(extractor.price_stats.is_empty());
        assert!(extractor.return_stats.is_empty());
    }

    #[test]
    fn test_feature_names() {
        let extractor = PriceExtractor::new();
        let names = extractor.feature_names();

        assert!(names.contains(&"volatility"));
        assert!(names.contains(&"z_score"));
        assert!(names.contains(&"ema_signal"));
        assert!(names.contains(&"momentum"));
    }
}
