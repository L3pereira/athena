//! Rolling statistics using integer arithmetic

use super::isqrt;
use serde::{Deserialize, Serialize};

/// Rolling statistics using integer arithmetic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingStats {
    /// Values in the window (scaled integers)
    values: Vec<i64>,
    /// Window size
    window_size: usize,
    /// Number of decimal places
    decimals: u8,
    /// Running sum for O(1) mean updates
    sum: i128,
}

impl RollingStats {
    /// Create a new rolling stats calculator
    pub fn new(window_size: usize, decimals: u8) -> Self {
        Self {
            values: Vec::with_capacity(window_size),
            window_size: window_size.max(1),
            decimals,
            sum: 0,
        }
    }

    /// Add a value (should be pre-scaled)
    #[inline]
    pub fn push(&mut self, value: i64) {
        if self.values.len() >= self.window_size {
            let removed = self.values.remove(0);
            self.sum -= removed as i128;
        }
        self.values.push(value);
        self.sum += value as i128;
    }

    /// Get current mean (scaled integer)
    #[inline]
    pub fn mean(&self) -> Option<i64> {
        if self.values.is_empty() {
            return None;
        }
        Some((self.sum / self.values.len() as i128) as i64)
    }

    /// Get current variance (scaled by 2x decimals due to squaring)
    pub fn variance(&self) -> Option<i128> {
        if self.values.len() < 2 {
            return None;
        }
        let mean = self.mean()? as i128;
        let sum_sq_diff: i128 = self
            .values
            .iter()
            .map(|&x| {
                let diff = x as i128 - mean;
                diff * diff
            })
            .sum();
        Some(sum_sq_diff / (self.values.len() - 1) as i128)
    }

    /// Get standard deviation (scaled by decimals)
    pub fn std_dev(&self) -> Option<i64> {
        self.variance().map(|v| isqrt(v) as i64)
    }

    /// Get z-score for a value (returns value scaled by decimals)
    /// Returns None if variance is zero or not enough data
    pub fn z_score(&self, value: i64) -> Option<i64> {
        let mean = self.mean()?;
        let std = self.std_dev()?;

        if std == 0 {
            return None;
        }

        let diff = value as i128 - mean as i128;
        let scale = 10i128.pow(self.decimals as u32);
        Some(((diff * scale) / std as i128) as i64)
    }

    /// Get the last value
    #[inline]
    pub fn last(&self) -> Option<i64> {
        self.values.last().copied()
    }

    /// Get the first value
    #[inline]
    pub fn first(&self) -> Option<i64> {
        self.values.first().copied()
    }

    /// Get number of values
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Check if window is full
    #[inline]
    pub fn is_full(&self) -> bool {
        self.values.len() >= self.window_size
    }

    /// Get the values
    #[inline]
    pub fn values(&self) -> &[i64] {
        &self.values
    }

    /// Get the number of decimal places
    #[inline]
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// Calculate momentum (scaled return from first to last)
    pub fn momentum(&self) -> Option<i64> {
        let first = self.first()?;
        let last = self.last()?;
        if first == 0 {
            return None;
        }
        let scale = 10i64.pow(self.decimals as u32);
        Some(((last as i128 - first as i128) * scale as i128 / first as i128) as i64)
    }

    /// Clear all values
    pub fn clear(&mut self) {
        self.values.clear();
        self.sum = 0;
    }

    /// Get the window size
    #[inline]
    pub fn window_size(&self) -> usize {
        self.window_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_stats_mean() {
        let mut stats = RollingStats::new(5, 8);

        for i in 1..=5 {
            stats.push(i * 100_000_000); // 1.0, 2.0, 3.0, 4.0, 5.0 scaled
        }

        assert!(stats.is_full());
        assert_eq!(stats.mean(), Some(300_000_000)); // 3.0 scaled

        // Add one more, window shifts
        stats.push(600_000_000); // 6.0 scaled
        assert_eq!(stats.len(), 5);
        assert_eq!(stats.mean(), Some(400_000_000)); // 4.0 scaled (2+3+4+5+6)/5
    }

    #[test]
    fn test_rolling_momentum() {
        let mut stats = RollingStats::new(5, 8);

        // 10, 20, 30, 40, 50 (scaled)
        for i in 1..=5 {
            stats.push(i * 10 * 100_000_000);
        }

        // Momentum = (50 - 10) / 10 = 4.0 = 400%
        let mom = stats.momentum().unwrap();
        assert_eq!(mom, 4 * 100_000_000); // 4.0 scaled
    }

    #[test]
    fn test_variance_constant() {
        let mut stats = RollingStats::new(5, 8);

        // All same values -> variance = 0
        for _ in 0..5 {
            stats.push(100_000_000);
        }
        assert_eq!(stats.variance(), Some(0));
        assert_eq!(stats.std_dev(), Some(0));
    }

    #[test]
    fn test_variance_varying() {
        let mut stats = RollingStats::new(5, 8);
        let values = [1, 2, 3, 4, 5];
        for v in values {
            stats.push(v * 100_000_000);
        }
        let var = stats.variance().unwrap();
        // Sample variance of [1,2,3,4,5] = 2.5
        // In scaled form: 2.5 * scale^2 = 2.5 * 10^16
        assert!(var > 0);
    }

    #[test]
    fn test_z_score() {
        let mut stats = RollingStats::new(10, 8);

        // Create data around 100 with some variance
        for v in [98, 99, 100, 101, 102, 99, 100, 101, 100, 99] {
            stats.push(v * 100_000_000);
        }

        // Z-score of the mean should be close to 0
        let mean = stats.mean().unwrap();
        let z = stats.z_score(mean);
        assert!(z.is_some());
        assert!(z.unwrap().abs() < 10_000_000); // Small z-score

        // Z-score of extreme value should be larger
        let z_high = stats.z_score(110 * 100_000_000);
        assert!(z_high.is_some());
        assert!(z_high.unwrap() > 100_000_000); // Positive, significant
    }

    #[test]
    fn test_rolling_clear() {
        let mut stats = RollingStats::new(5, 8);
        stats.push(100_000_000);
        assert!(!stats.is_empty());

        stats.clear();
        assert!(stats.is_empty());
        assert!(stats.mean().is_none());
    }
}
