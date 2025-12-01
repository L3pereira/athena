use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::values::{Price, Quantity};

/// A spot trading pair (e.g., BTC/USD, ETH/BTC)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpotPair {
    /// Base currency (the one being bought/sold)
    pub base: String,
    /// Quote currency (the one used to price the base)
    pub quote: String,
    /// Minimum price increment
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
}

impl SpotPair {
    /// Create a new spot pair with default tick and lot sizes
    pub fn new(base: impl Into<String>, quote: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            tick_size: dec!(0.01),
            lot_size: dec!(0.001),
        }
    }

    /// Create a spot pair with custom specifications
    pub fn with_specs(
        base: impl Into<String>,
        quote: impl Into<String>,
        tick_size: Price,
        lot_size: Quantity,
    ) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            tick_size,
            lot_size,
        }
    }

    /// Common crypto pairs
    pub fn btc_usd() -> Self {
        Self::with_specs("BTC", "USD", dec!(0.01), dec!(0.00001))
    }

    pub fn eth_usd() -> Self {
        Self::with_specs("ETH", "USD", dec!(0.01), dec!(0.0001))
    }

    pub fn eth_btc() -> Self {
        Self::with_specs("ETH", "BTC", dec!(0.00001), dec!(0.001))
    }
}

impl InstrumentSpec for SpotPair {
    fn symbol(&self) -> &str {
        // We need to store the formatted symbol
        // For now, this is a limitation - we'll return base as symbol
        // In production, you'd cache the formatted string
        &self.base
    }

    fn tick_size(&self) -> Price {
        self.tick_size
    }

    fn lot_size(&self) -> Quantity {
        self.lot_size
    }

    fn margin_requirement(&self) -> Decimal {
        // Spot trading typically requires 100% margin (no leverage)
        Decimal::ONE
    }

    fn is_shortable(&self) -> bool {
        // Spot can only be shorted if you have the asset to sell
        // For simulation purposes, we'll allow it
        true
    }
}

impl std::fmt::Display for SpotPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.base, self.quote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spot_pair_creation() {
        let btc_usd = SpotPair::btc_usd();
        assert_eq!(btc_usd.base, "BTC");
        assert_eq!(btc_usd.quote, "USD");
        assert_eq!(btc_usd.tick_size, dec!(0.01));
        assert_eq!(btc_usd.lot_size, dec!(0.00001));
    }

    #[test]
    fn test_spot_display() {
        let pair = SpotPair::new("BTC", "USD");
        assert_eq!(format!("{}", pair), "BTC/USD");
    }

    #[test]
    fn test_spot_margin() {
        let pair = SpotPair::btc_usd();
        assert_eq!(pair.margin_requirement(), Decimal::ONE);
        assert_eq!(pair.max_leverage(), Decimal::ONE);
    }

    #[test]
    fn test_price_validation() {
        let pair = SpotPair::with_specs("BTC", "USD", dec!(0.01), dec!(0.001));
        assert!(pair.validate_price(dec!(50000.01)));
        assert!(pair.validate_price(dec!(50000.00)));
        assert!(!pair.validate_price(dec!(50000.001)));
    }

    #[test]
    fn test_quantity_validation() {
        let pair = SpotPair::with_specs("BTC", "USD", dec!(0.01), dec!(0.001));
        assert!(pair.validate_quantity(dec!(1.001)));
        assert!(pair.validate_quantity(dec!(1.000)));
        assert!(!pair.validate_quantity(dec!(1.0001)));
    }
}
