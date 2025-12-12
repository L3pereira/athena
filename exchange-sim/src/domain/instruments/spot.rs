use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::domain::{Price, Quantity, Rate, Value};

/// A spot trading pair (e.g., BTC/USDT)
///
/// Represents immediate delivery of assets at the current market price.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpotPair {
    /// Base asset (e.g., BTC)
    pub base: String,
    /// Quote asset (e.g., USDT)
    pub quote: String,
    /// Trading pair symbol (e.g., BTCUSDT)
    pub symbol: String,
    /// Minimum price increment
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
    /// Minimum notional value (price * qty)
    pub min_notional: Value,
}

impl SpotPair {
    pub fn new(base: impl Into<String>, quote: impl Into<String>) -> Self {
        let base = base.into();
        let quote = quote.into();
        let symbol = format!("{}{}", base, quote);

        Self {
            base,
            quote,
            symbol,
            tick_size: Price::from_f64(0.01),
            lot_size: Quantity::from_f64(0.001),
            min_notional: Value::from_int(10),
        }
    }

    pub fn with_tick_size(mut self, tick: Price) -> Self {
        self.tick_size = tick;
        self
    }

    pub fn with_lot_size(mut self, lot: Quantity) -> Self {
        self.lot_size = lot;
        self
    }

    pub fn with_min_notional(mut self, min: Value) -> Self {
        self.min_notional = min;
        self
    }

    /// Common spot pairs
    pub fn btcusdt() -> Self {
        Self::new("BTC", "USDT")
            .with_tick_size(Price::from_f64(0.01))
            .with_lot_size(Quantity::from_f64(0.00001))
            .with_min_notional(Value::from_int(5))
    }

    pub fn ethusdt() -> Self {
        Self::new("ETH", "USDT")
            .with_tick_size(Price::from_f64(0.01))
            .with_lot_size(Quantity::from_f64(0.0001))
            .with_min_notional(Value::from_int(5))
    }

    pub fn solusdt() -> Self {
        Self::new("SOL", "USDT")
            .with_tick_size(Price::from_f64(0.001))
            .with_lot_size(Quantity::from_f64(0.01))
            .with_min_notional(Value::from_int(5))
    }
}

impl InstrumentSpec for SpotPair {
    fn symbol(&self) -> &str {
        &self.symbol
    }

    fn tick_size(&self) -> Price {
        self.tick_size
    }

    fn lot_size(&self) -> Quantity {
        self.lot_size
    }

    fn base_asset(&self) -> &str {
        &self.base
    }

    fn quote_asset(&self) -> &str {
        &self.quote
    }

    fn margin_requirement(&self) -> Rate {
        Rate::ONE // Spot = 100% margin (no leverage)
    }

    fn is_shortable(&self) -> bool {
        false // Spot cannot be shorted without margin
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
    fn test_spot_creation() {
        let pair = SpotPair::btcusdt();
        assert_eq!(pair.base, "BTC");
        assert_eq!(pair.quote, "USDT");
        assert_eq!(pair.symbol, "BTCUSDT");
    }

    #[test]
    fn test_price_validation() {
        let pair = SpotPair::new("BTC", "USDT").with_tick_size(Price::from_f64(0.01));
        assert!(pair.validate_price(Price::from_f64(100.01)));
        // Note: 100.001 won't pass validation with tick_size 0.01
        assert!(!pair.validate_price(Price::from_f64(100.005)));
    }

    #[test]
    fn test_quantity_validation() {
        let lot_size = Quantity::from_raw(100_000); // 0.001 * 100_000_000
        let pair = SpotPair::new("BTC", "USDT").with_lot_size(lot_size);
        // 1.001 with lot_size 0.001 should be valid (1001 lots)
        let valid_qty = Quantity::from_raw(100_100_000); // 1.001 * 100_000_000
        assert!(pair.validate_quantity(valid_qty));
        // 1.0005 should fail validation with lot_size 0.001
        let invalid_qty = Quantity::from_raw(100_050_000); // 1.0005 * 100_000_000
        assert!(!pair.validate_quantity(invalid_qty));
    }
}
