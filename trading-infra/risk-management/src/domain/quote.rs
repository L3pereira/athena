//! Quote Types
//!
//! Two-sided market maker quotes.

use serde::{Deserialize, Serialize};
use trading_core::{Price, Quantity};

/// Two-sided market maker quote
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    /// Bid price
    pub bid_price: Price,
    /// Ask price
    pub ask_price: Price,
    /// Bid size
    pub bid_size: Quantity,
    /// Ask size
    pub ask_size: Quantity,
}

impl Quote {
    pub fn new(bid_price: Price, ask_price: Price, bid_size: Quantity, ask_size: Quantity) -> Self {
        Self {
            bid_price,
            ask_price,
            bid_size,
            ask_size,
        }
    }

    /// Create a symmetric quote around mid price
    pub fn symmetric(mid: Price, half_spread: Price, size: Quantity) -> Self {
        Self {
            bid_price: mid - half_spread,
            ask_price: mid + half_spread,
            bid_size: size,
            ask_size: size,
        }
    }

    /// Get the mid price
    pub fn mid_price(&self) -> Price {
        Price::from_raw((self.bid_price.raw() + self.ask_price.raw()) / 2)
    }

    /// Get the spread
    pub fn spread(&self) -> Price {
        self.ask_price - self.bid_price
    }

    /// Get spread in basis points
    pub fn spread_bps(&self) -> f64 {
        let mid = self.mid_price().raw() as f64;
        if mid == 0.0 {
            return 0.0;
        }
        (self.spread().raw() as f64 / mid) * 10_000.0
    }

    /// Check if quote is crossed (invalid)
    pub fn is_crossed(&self) -> bool {
        self.bid_price >= self.ask_price
    }

    /// Check if either side is empty
    pub fn is_one_sided(&self) -> bool {
        self.bid_size.is_zero() || self.ask_size.is_zero()
    }

    /// Apply inventory skew
    /// Positive skew shifts quotes down (encourages selling to you)
    /// Negative skew shifts quotes up (encourages buying from you)
    pub fn with_skew(mut self, skew_bps: f64) -> Self {
        let skew_raw = (self.mid_price().raw() as f64 * skew_bps / 10_000.0) as i64;
        self.bid_price = Price::from_raw(self.bid_price.raw() - skew_raw);
        self.ask_price = Price::from_raw(self.ask_price.raw() - skew_raw);
        self
    }

    /// Widen spread by a multiplier
    pub fn with_widening(self, multiplier: f64) -> Self {
        let mid = self.mid_price();
        let current_half_spread = self.spread().raw() / 2;
        let new_half_spread = (current_half_spread as f64 * multiplier) as i64;

        Self {
            bid_price: Price::from_raw(mid.raw() - new_half_spread),
            ask_price: Price::from_raw(mid.raw() + new_half_spread),
            bid_size: self.bid_size,
            ask_size: self.ask_size,
        }
    }

    /// Reduce depth by a multiplier
    pub fn with_reduced_depth(mut self, multiplier: f64) -> Self {
        let bid_raw = (self.bid_size.raw() as f64 * multiplier) as i64;
        let ask_raw = (self.ask_size.raw() as f64 * multiplier) as i64;
        self.bid_size = Quantity::from_raw(bid_raw.max(0));
        self.ask_size = Quantity::from_raw(ask_raw.max(0));
        self
    }
}

impl Default for Quote {
    fn default() -> Self {
        Self {
            bid_price: Price::ZERO,
            ask_price: Price::ZERO,
            bid_size: Quantity::ZERO,
            ask_size: Quantity::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symmetric_quote() {
        let quote = Quote::symmetric(
            Price::from_int(100),
            Price::from_raw(5_000_000), // 0.05 half spread
            Quantity::from_int(10),
        );

        assert!((quote.mid_price().raw() - 100_00000000).abs() < 100);
        assert_eq!(quote.spread().raw(), 10_000_000);
    }

    #[test]
    fn test_spread_bps() {
        let quote = Quote::symmetric(
            Price::from_int(100),
            Price::from_raw(5_000_000), // 0.05 = 5 bps half spread
            Quantity::from_int(10),
        );

        assert!((quote.spread_bps() - 10.0).abs() < 0.1); // 10 bps total
    }

    #[test]
    fn test_skew() {
        let quote = Quote::symmetric(
            Price::from_int(100),
            Price::from_raw(5_000_000),
            Quantity::from_int(10),
        );

        // Skew down by 5 bps (encourages selling)
        let skewed = quote.with_skew(5.0);

        assert!(skewed.mid_price().raw() < quote.mid_price().raw());
    }

    #[test]
    fn test_crossed_detection() {
        let valid = Quote::symmetric(
            Price::from_int(100),
            Price::from_raw(5_000_000),
            Quantity::from_int(1),
        );
        assert!(!valid.is_crossed());

        let crossed = Quote::new(
            Price::from_int(100),
            Price::from_int(99),
            Quantity::from_int(1),
            Quantity::from_int(1),
        );
        assert!(crossed.is_crossed());
    }
}
