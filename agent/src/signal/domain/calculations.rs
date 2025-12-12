//! Pure domain calculations for market data analysis
//!
//! These are stateless functions that operate on raw data.
//! No external dependencies - pure business logic.

use super::value_objects::{BasisPoints, Ratio};
use trading_core::{Price, Quantity};

/// Collection of pure market data calculations
pub struct Calculations;

impl Calculations {
    /// Calculate microprice (volume-weighted mid price)
    ///
    /// microprice = (bid_price * ask_size + ask_price * bid_size) / (bid_size + ask_size)
    pub fn microprice(microprice: Microprice) -> Option<Price> {
        microprice.calculate()
    }

    /// Calculate order book imbalance
    ///
    /// imbalance = (bid_size - ask_size) / (bid_size + ask_size)
    /// Returns a Ratio in [-1, 1]
    pub fn imbalance(imbalance: Imbalance) -> Ratio {
        imbalance.calculate()
    }

    /// Calculate spread
    pub fn spread(spread: Spread) -> Price {
        spread.calculate()
    }

    /// Calculate VWAP for a set of price levels
    pub fn vwap(vwap: Vwap) -> Option<Price> {
        vwap.calculate()
    }
}

/// Input for microprice calculation
#[derive(Debug, Clone)]
pub struct Microprice {
    pub best_bid: Price,
    pub best_ask: Price,
    pub bid_size: Quantity,
    pub ask_size: Quantity,
}

impl Microprice {
    pub fn new(best_bid: Price, best_ask: Price, bid_size: Quantity, ask_size: Quantity) -> Self {
        Self {
            best_bid,
            best_ask,
            bid_size,
            ask_size,
        }
    }

    /// Calculate the microprice
    pub fn calculate(&self) -> Option<Price> {
        let total_size = self.bid_size.raw() as i128 + self.ask_size.raw() as i128;
        if total_size == 0 {
            return None;
        }
        // microprice = (bid * ask_size + ask * bid_size) / total_size
        let numerator = self.best_bid.raw() as i128 * self.ask_size.raw() as i128
            + self.best_ask.raw() as i128 * self.bid_size.raw() as i128;
        let microprice_raw = numerator / total_size;
        Some(Price::from_raw(microprice_raw as i64))
    }

    /// Calculate microprice skew from mid price (in basis points)
    pub fn skew_bps(&self) -> Option<BasisPoints> {
        let mid_raw = (self.best_bid.raw() + self.best_ask.raw()) / 2;
        if mid_raw == 0 {
            return None;
        }

        let microprice = self.calculate()?;
        let mid = Price::from_raw(mid_raw);
        BasisPoints::from_price_diff(microprice, mid)
    }

    /// Calculate microprice skew as f64 (backward compatibility)
    pub fn skew_bps_value(&self) -> Option<f64> {
        self.skew_bps().map(|b| b.value())
    }
}

/// Input for imbalance calculation
#[derive(Debug, Clone)]
pub struct Imbalance {
    pub bid_size: Quantity,
    pub ask_size: Quantity,
}

impl Imbalance {
    pub fn new(bid_size: Quantity, ask_size: Quantity) -> Self {
        Self { bid_size, ask_size }
    }

    /// Calculate the imbalance as a Ratio [-1, 1]
    /// Positive = more bids (buying pressure)
    /// Negative = more asks (selling pressure)
    pub fn calculate(&self) -> Ratio {
        Ratio::imbalance(self.bid_size, self.ask_size)
    }

    /// Get raw f64 value for backward compatibility
    pub fn value(&self) -> f64 {
        self.calculate().value()
    }

    /// Returns true if there's buying pressure (positive imbalance)
    pub fn is_buying_pressure(&self) -> bool {
        self.calculate().value() > 0.0
    }

    /// Returns true if there's selling pressure (negative imbalance)
    pub fn is_selling_pressure(&self) -> bool {
        self.calculate().value() < 0.0
    }
}

/// Input for spread calculation
#[derive(Debug, Clone)]
pub struct Spread {
    pub best_bid: Price,
    pub best_ask: Price,
}

impl Spread {
    pub fn new(best_bid: Price, best_ask: Price) -> Self {
        Self { best_bid, best_ask }
    }

    /// Calculate absolute spread
    pub fn calculate(&self) -> Price {
        Price::from_raw(self.best_ask.raw() - self.best_bid.raw())
    }

    /// Calculate spread in basis points
    pub fn bps(&self) -> Option<BasisPoints> {
        let mid = self.mid_price();
        if mid.is_zero() {
            return None;
        }
        let spread = self.calculate();
        // spread_bps = (spread / mid) * 10000
        let spread_bps = (spread.raw() as i128 * 10000) / mid.raw() as i128;
        Some(BasisPoints::new(spread_bps as f64))
    }

    /// Calculate spread in basis points as f64 (backward compatibility)
    pub fn bps_value(&self) -> Option<f64> {
        self.bps().map(|b| b.value())
    }

    /// Calculate mid price
    pub fn mid_price(&self) -> Price {
        Price::from_raw((self.best_bid.raw() + self.best_ask.raw()) / 2)
    }
}

/// Input for VWAP calculation
#[derive(Debug, Clone)]
pub struct Vwap {
    levels: Vec<(Price, Quantity)>,
}

impl Vwap {
    pub fn new() -> Self {
        Self { levels: Vec::new() }
    }

    pub fn from_levels(levels: Vec<(Price, Quantity)>) -> Self {
        Self { levels }
    }

    /// Add a price level
    pub fn add_level(&mut self, price: Price, size: Quantity) {
        self.levels.push((price, size));
    }

    /// Calculate VWAP
    pub fn calculate(&self) -> Option<Price> {
        let mut total_value: i128 = 0;
        let mut total_size: i128 = 0;

        for (price, size) in &self.levels {
            total_value += price.raw() as i128 * size.raw() as i128;
            total_size += size.raw() as i128;
        }

        if total_size == 0 {
            None
        } else {
            // VWAP = total_value / total_size
            let vwap_raw = total_value / total_size;
            Some(Price::from_raw(vwap_raw as i64))
        }
    }

    /// Calculate VWAP for top N levels
    pub fn calculate_top(&self, n: usize) -> Option<Price> {
        let mut total_value: i128 = 0;
        let mut total_size: i128 = 0;

        for (price, size) in self.levels.iter().take(n) {
            total_value += price.raw() as i128 * size.raw() as i128;
            total_size += size.raw() as i128;
        }

        if total_size == 0 {
            None
        } else {
            let vwap_raw = total_value / total_size;
            Some(Price::from_raw(vwap_raw as i64))
        }
    }

    /// Get total depth (sum of sizes)
    pub fn total_depth(&self) -> Quantity {
        let sum: i64 = self.levels.iter().map(|(_, size)| size.raw()).sum();
        Quantity::from_raw(sum)
    }

    /// Get total depth for top N levels
    pub fn total_depth_top(&self, n: usize) -> Quantity {
        let sum: i64 = self.levels.iter().take(n).map(|(_, size)| size.raw()).sum();
        Quantity::from_raw(sum)
    }
}

impl Default for Vwap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microprice() {
        // bid=100 size=10, ask=101 size=5
        // microprice = (100*5 + 101*10) / (10+5) = (500 + 1010) / 15 = 100.666...
        let mp = Microprice::new(
            Price::from_int(100),
            Price::from_int(101),
            Quantity::from_int(10),
            Quantity::from_int(5),
        );

        let result = mp.calculate().unwrap();
        assert!(result.to_f64() > 100.0);
        assert!(result.to_f64() < 101.0);
    }

    #[test]
    fn test_imbalance() {
        // More bids than asks -> positive
        let imb = Imbalance::new(Quantity::from_int(10), Quantity::from_int(5));
        assert!(imb.value() > 0.0);
        assert!(imb.is_buying_pressure());
        assert!((imb.value() - 0.333).abs() < 0.01);

        // More asks than bids -> negative
        let imb = Imbalance::new(Quantity::from_int(5), Quantity::from_int(10));
        assert!(imb.value() < 0.0);
        assert!(imb.is_selling_pressure());

        // Equal -> zero
        let imb = Imbalance::new(Quantity::from_int(10), Quantity::from_int(10));
        assert_eq!(imb.value(), 0.0);
    }

    #[test]
    fn test_spread() {
        let spread = Spread::new(Price::from_int(100), Price::from_int(101));

        assert!((spread.calculate().to_f64() - 1.0).abs() < 0.01);
        assert!((spread.mid_price().to_f64() - 100.5).abs() < 0.01);

        // Spread bps = 1 / 100.5 * 10000 ≈ 99.5
        let bps = spread.bps().unwrap();
        assert!((bps.value() - 99.5).abs() < 1.0);
    }

    #[test]
    fn test_vwap() {
        let mut vwap = Vwap::new();
        vwap.add_level(Price::from_int(100), Quantity::from_int(10));
        vwap.add_level(Price::from_int(99), Quantity::from_int(20));
        vwap.add_level(Price::from_int(98), Quantity::from_int(30));

        // VWAP = (100*10 + 99*20 + 98*30) / (10+20+30)
        //      = (1000 + 1980 + 2940) / 60 = 5920 / 60 = 98.666...
        let result = vwap.calculate().unwrap();
        assert!(result.to_f64() > 98.0);
        assert!(result.to_f64() < 99.0);

        // Total depth
        assert!((vwap.total_depth().to_f64() - 60.0).abs() < 0.01);
        assert!((vwap.total_depth_top(2).to_f64() - 30.0).abs() < 0.01);
    }
}
