//! Price-based feature extraction
//!
//! Extracts features from price history and returns calculations.
//! Uses trading-core's integer-based statistics for precision.

use crate::order_book::SharedOrderBook;
use crate::signal::domain::{Volatility, ZScore};
use crate::signal::traits::FeatureExtractor;
use std::collections::HashMap;
use trading_core::{Ema, PRICE_SCALE, Price, RollingStats};

/// Price-based feature extractor with rolling statistics
///
/// Maintains a window of recent prices and calculates:
/// - `price_return`: Simple return from last price
/// - `log_return`: Log return from last price
/// - `volatility`: Rolling standard deviation of returns (as `Volatility`)
/// - `z_score`: Current price's z-score relative to rolling mean/std (as `ZScore`)
/// - `ema_fast`: Exponential moving average (fast) as `Price`
/// - `ema_slow`: Exponential moving average (slow) as `Price`
/// - `ema_signal`: EMA crossover signal (fast - slow)
/// - `momentum`: Price change over lookback period
#[derive(Debug, Clone)]
pub struct PriceFeatures {
    /// Rolling statistics for prices
    price_stats: RollingStats,
    /// Rolling statistics for returns (scaled by PRICE_SCALE)
    return_stats: RollingStats,
    /// Fast EMA
    ema_fast: Ema,
    /// Slow EMA
    ema_slow: Ema,
    /// Last price (scaled)
    last_price: Option<i64>,
    /// Last computed volatility (typed)
    last_volatility: Option<Volatility>,
    /// Last computed z-score (typed)
    last_z_score: Option<ZScore>,
}

impl PriceFeatures {
    /// Create with default parameters
    pub fn new() -> Self {
        Self::with_params(20, 5, 20)
    }

    /// Create with custom parameters
    pub fn with_params(window_size: usize, ema_fast_period: usize, ema_slow_period: usize) -> Self {
        Self {
            price_stats: RollingStats::new(window_size.max(2), 8),
            return_stats: RollingStats::new(window_size.max(2), 8),
            ema_fast: Ema::from_period(ema_fast_period.max(2), 8),
            ema_slow: Ema::from_period(ema_slow_period.max(2), 8),
            last_price: None,
            last_volatility: None,
            last_z_score: None,
        }
    }

    // === Typed accessors ===

    /// Get the fast EMA as a Price
    pub fn ema_fast_price(&self) -> Option<Price> {
        self.ema_fast.value_i64().map(Price::from_raw)
    }

    /// Get the slow EMA as a Price
    pub fn ema_slow_price(&self) -> Option<Price> {
        self.ema_slow.value_i64().map(Price::from_raw)
    }

    /// Get the last computed volatility
    pub fn volatility(&self) -> Option<Volatility> {
        self.last_volatility
    }

    /// Get the last computed z-score
    pub fn z_score(&self) -> Option<ZScore> {
        self.last_z_score
    }

    /// Update state with new price and return features
    pub fn update(&mut self, mid_price: f64) -> HashMap<String, f64> {
        let mut features = HashMap::new();

        // Convert to scaled integer
        let price_raw = (mid_price * PRICE_SCALE as f64) as i64;

        // Calculate return if we have history
        if let Some(last_raw) = self.last_price
            && last_raw != 0
        {
            // Simple return: (new - old) / old, scaled by PRICE_SCALE
            let simple_return_scaled = ((price_raw as i128 - last_raw as i128)
                * PRICE_SCALE as i128
                / last_raw as i128) as i64;

            // Log return (use f64 for ln, then scale back)
            let log_return = (mid_price / (last_raw as f64 / PRICE_SCALE as f64)).ln();
            let log_return_scaled = (log_return * PRICE_SCALE as f64) as i64;

            features.insert(
                "price_return".to_string(),
                simple_return_scaled as f64 / PRICE_SCALE as f64,
            );
            features.insert("log_return".to_string(), log_return);

            // Store return for volatility calculation
            self.return_stats.push(log_return_scaled);
        }

        // Store price
        self.price_stats.push(price_raw);
        self.last_price = Some(price_raw);

        // Update EMAs
        self.ema_fast.update(price_raw);
        self.ema_slow.update(price_raw);

        // Calculate volatility from returns
        if let Some(std_dev) = self.return_stats.std_dev() {
            let volatility_val = std_dev as f64 / PRICE_SCALE as f64;
            self.last_volatility = Some(Volatility::new(volatility_val));
            features.insert("volatility".to_string(), volatility_val);
        }

        // Calculate z-score for current price
        if let Some(z) = self.price_stats.z_score(price_raw) {
            let z_val = z as f64 / PRICE_SCALE as f64;
            self.last_z_score = Some(ZScore::new(z_val));
            features.insert("z_score".to_string(), z_val);
        }

        // Mean return
        if let Some(mean) = self.return_stats.mean() {
            features.insert("mean_return".to_string(), mean as f64 / PRICE_SCALE as f64);
        }

        // EMA features
        if let (Some(fast), Some(slow)) = (self.ema_fast.value_i64(), self.ema_slow.value_i64()) {
            let fast_f64 = fast as f64 / PRICE_SCALE as f64;
            let slow_f64 = slow as f64 / PRICE_SCALE as f64;

            features.insert("ema_fast".to_string(), fast_f64);
            features.insert("ema_slow".to_string(), slow_f64);

            // EMA signal: positive when fast > slow (bullish)
            let ema_signal = fast_f64 - slow_f64;
            features.insert("ema_signal".to_string(), ema_signal);

            // Normalized signal
            if slow_f64.abs() > 1e-10 {
                let ema_signal_pct = ema_signal / slow_f64;
                features.insert("ema_signal_pct".to_string(), ema_signal_pct);
            }
        }

        // Momentum
        if let Some(momentum) = self.price_stats.momentum() {
            features.insert("momentum".to_string(), momentum as f64 / PRICE_SCALE as f64);
        }

        features
    }

    /// Reset state
    pub fn reset(&mut self) {
        self.price_stats.clear();
        self.return_stats.clear();
        self.ema_fast.reset();
        self.ema_slow.reset();
        self.last_price = None;
        self.last_volatility = None;
        self.last_z_score = None;
    }
}

impl Default for PriceFeatures {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureExtractor for PriceFeatures {
    fn extract(&self, book: &SharedOrderBook) -> HashMap<String, f64> {
        // Note: This is stateless extraction from a single book snapshot
        // For stateful features (EMA, rolling stats), use the update() method directly

        let mut features = HashMap::new();

        if let Some(mid) = book.mid_price() {
            features.insert("current_mid".to_string(), mid.to_f64());

            // Include current EMA state if available
            if let Some(fast) = self.ema_fast.value_i64() {
                features.insert("ema_fast".to_string(), fast as f64 / PRICE_SCALE as f64);
            }
            if let Some(slow) = self.ema_slow.value_i64() {
                features.insert("ema_slow".to_string(), slow as f64 / PRICE_SCALE as f64);
            }
        }

        features
    }

    fn feature_names(&self) -> &[&str] {
        &[
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
            "current_mid",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_features_update() {
        let mut pf = PriceFeatures::new();

        // First update - no return yet
        let features = pf.update(100.0);
        assert!(!features.contains_key("price_return"));
        assert!(features.contains_key("ema_fast"));

        // Second update - should have return
        let features = pf.update(101.0);
        let ret = features.get("price_return").unwrap();
        assert!((ret - 0.01).abs() < 0.001); // 1% return

        // After enough updates, we should have volatility
        for i in 0..20 {
            pf.update(100.0 + (i as f64 % 3.0));
        }
        let features = pf.update(102.0);
        assert!(features.contains_key("volatility"));
        assert!(features.contains_key("z_score"));
    }

    #[test]
    fn test_ema_calculation() {
        let mut pf = PriceFeatures::with_params(10, 3, 10);

        // Feed constant prices
        for _ in 0..20 {
            pf.update(100.0);
        }

        // EMAs should converge to price
        let fast = pf.ema_fast.value_i64().unwrap() as f64 / PRICE_SCALE as f64;
        let slow = pf.ema_slow.value_i64().unwrap() as f64 / PRICE_SCALE as f64;
        assert!((fast - 100.0).abs() < 0.1);
        assert!((slow - 100.0).abs() < 0.1);

        // Now feed higher prices - fast should react quicker
        for _ in 0..5 {
            pf.update(110.0);
        }

        let fast = pf.ema_fast.value_i64().unwrap() as f64 / PRICE_SCALE as f64;
        let slow = pf.ema_slow.value_i64().unwrap() as f64 / PRICE_SCALE as f64;
        // Fast EMA should be closer to 110 than slow EMA
        assert!(fast > slow);
    }

    #[test]
    fn test_z_score() {
        let mut pf = PriceFeatures::with_params(10, 5, 10);

        // Feed prices around 100
        let prices = [
            98.0, 99.0, 100.0, 101.0, 102.0, 99.0, 100.0, 101.0, 100.0, 99.0,
        ];
        for &p in &prices {
            pf.update(p);
        }

        // Update with 100 - should have z-score close to 0 (near mean)
        let features = pf.update(100.0);
        if let Some(z) = features.get("z_score") {
            assert!(z.is_finite(), "z_score should be finite");
            // Mean is ~100, so z-score of 100 should be near 0
            assert!(
                z.abs() < 1.5,
                "z_score of mean value should be small, got {}",
                z
            );
        }

        // Update with extreme value - z-score should be larger
        let features = pf.update(105.0);
        if let Some(z) = features.get("z_score") {
            assert!(z.is_finite(), "z_score should be finite");
            assert!(
                *z > 1.0,
                "z_score of high value should be positive, got {}",
                z
            );
        }
    }

    #[test]
    fn test_momentum() {
        let mut pf = PriceFeatures::with_params(5, 3, 5);

        // Start at 100
        for _ in 0..5 {
            pf.update(100.0);
        }

        // Move to 110 - 10% momentum
        let features = pf.update(110.0);
        if let Some(mom) = features.get("momentum") {
            assert!((mom - 0.10).abs() < 0.05);
        }
    }

    #[test]
    fn test_volatility() {
        let mut pf = PriceFeatures::with_params(10, 5, 10);

        // Feed constant prices - volatility should be low
        for _ in 0..10 {
            pf.update(100.0);
        }
        let features = pf.update(100.0);
        if let Some(vol) = features.get("volatility") {
            assert!(
                vol.abs() < 0.001,
                "constant prices should have near-zero volatility"
            );
        }

        // Feed varying prices - volatility should increase
        let mut pf2 = PriceFeatures::with_params(10, 5, 10);
        for i in 0..10 {
            pf2.update(100.0 + (i as f64 * 2.0) * if i % 2 == 0 { 1.0 } else { -1.0 });
        }
        let features2 = pf2.update(100.0);
        if let Some(vol) = features2.get("volatility") {
            assert!(
                *vol > 0.01,
                "varying prices should have positive volatility"
            );
        }
    }
}
