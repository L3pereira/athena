//! Exponential Moving Average using integer arithmetic

use serde::{Deserialize, Serialize};

/// EMA (Exponential Moving Average) calculator using integer arithmetic
///
/// Uses rational representation for alpha to avoid floating-point:
/// alpha = alpha_num / alpha_denom
///
/// EMA formula: new_ema = (alpha_num * value + (alpha_denom - alpha_num) * old_ema) / alpha_denom
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ema {
    /// Current EMA value (scaled integer)
    value: i128,
    /// Number of decimal places in the value
    decimals: u8,
    /// Alpha numerator (e.g., 2 for period-based EMA)
    alpha_num: i64,
    /// Alpha denominator (e.g., period + 1 for period-based EMA)
    alpha_denom: i64,
    /// Whether the EMA has been initialized with at least one value
    initialized: bool,
}

impl Ema {
    /// Create EMA from a period (standard formula: alpha = 2 / (period + 1))
    pub fn from_period(period: usize, decimals: u8) -> Self {
        Self {
            value: 0,
            decimals,
            alpha_num: 2,
            alpha_denom: period as i64 + 1,
            initialized: false,
        }
    }

    /// Create EMA with custom alpha as rational number
    pub fn with_alpha(alpha_num: i64, alpha_denom: i64, decimals: u8) -> Self {
        Self {
            value: 0,
            decimals,
            alpha_num,
            alpha_denom,
            initialized: false,
        }
    }

    /// Update EMA with new value (value should be pre-scaled)
    #[inline]
    pub fn update(&mut self, value: i64) {
        if !self.initialized {
            self.value = value as i128;
            self.initialized = true;
        } else {
            // EMA = (alpha * value + (1 - alpha) * old_ema)
            // Using integers: (alpha_num * value + (alpha_denom - alpha_num) * old_ema) / alpha_denom
            let alpha_complement = self.alpha_denom - self.alpha_num;
            self.value = (self.alpha_num as i128 * value as i128
                + alpha_complement as i128 * self.value)
                / self.alpha_denom as i128;
        }
    }

    /// Get current EMA value (scaled integer)
    #[inline]
    pub fn value(&self) -> Option<i128> {
        if self.initialized {
            Some(self.value)
        } else {
            None
        }
    }

    /// Get current EMA value as i64 (may overflow for large values)
    #[inline]
    pub fn value_i64(&self) -> Option<i64> {
        self.value().map(|v| v as i64)
    }

    /// Check if initialized
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get the number of decimals
    #[inline]
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// Get the alpha numerator
    #[inline]
    pub fn alpha_num(&self) -> i64 {
        self.alpha_num
    }

    /// Get the alpha denominator
    #[inline]
    pub fn alpha_denom(&self) -> i64 {
        self.alpha_denom
    }

    /// Reset the EMA
    pub fn reset(&mut self) {
        self.value = 0;
        self.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ema_period() {
        // Period 9 EMA: alpha = 2/10 = 0.2
        let mut ema = Ema::from_period(9, 8);

        // First value initializes
        ema.update(100_000_000); // 1.0 scaled
        assert_eq!(ema.value_i64(), Some(100_000_000));

        // Second value: EMA = 0.2 * 2.0 + 0.8 * 1.0 = 1.2
        ema.update(200_000_000); // 2.0 scaled
        let expected = (2 * 200_000_000 + 8 * 100_000_000) / 10; // 120_000_000
        assert_eq!(ema.value_i64(), Some(expected));
    }

    #[test]
    fn test_ema_convergence() {
        let mut ema = Ema::from_period(9, 8);

        // Feed constant value - EMA should converge to it
        for _ in 0..50 {
            ema.update(100_000_000);
        }
        assert_eq!(ema.value_i64(), Some(100_000_000));
    }

    #[test]
    fn test_ema_custom_alpha() {
        // alpha = 1/4 = 0.25
        let mut ema = Ema::with_alpha(1, 4, 8);

        ema.update(100_000_000);
        ema.update(200_000_000);
        // EMA = 0.25 * 200 + 0.75 * 100 = 125
        let expected = (200_000_000 + 3 * 100_000_000) / 4;
        assert_eq!(ema.value_i64(), Some(expected));
    }

    #[test]
    fn test_ema_reset() {
        let mut ema = Ema::from_period(9, 8);
        ema.update(100_000_000);
        assert!(ema.is_initialized());

        ema.reset();
        assert!(!ema.is_initialized());
        assert!(ema.value().is_none());
    }
}
