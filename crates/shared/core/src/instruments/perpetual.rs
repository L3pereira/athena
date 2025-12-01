use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::values::{Price, Quantity};

/// A perpetual swap contract (e.g., BTC-PERP)
///
/// Perpetuals are like futures but without expiry. They use a funding rate
/// mechanism to keep the contract price aligned with the underlying spot price.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PerpetualContract {
    /// Underlying asset
    pub underlying: String,
    /// Contract symbol (e.g., "BTC-PERP")
    pub symbol: String,
    /// Minimum price increment
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
    /// Contract multiplier
    pub multiplier: Decimal,
    /// Initial margin requirement
    pub initial_margin: Decimal,
    /// Funding interval in hours (typically 8)
    pub funding_interval_hours: u32,
    /// Whether this is an inverse contract (settled in crypto)
    pub is_inverse: bool,
}

impl PerpetualContract {
    /// Create a new linear perpetual contract (settled in quote currency)
    pub fn new_linear(underlying: impl Into<String>, symbol: impl Into<String>) -> Self {
        Self {
            underlying: underlying.into(),
            symbol: symbol.into(),
            tick_size: dec!(0.01),
            lot_size: dec!(0.001),
            multiplier: dec!(1),
            initial_margin: dec!(0.01), // 1% = 100x max leverage
            funding_interval_hours: 8,
            is_inverse: false,
        }
    }

    /// Create a new inverse perpetual contract (settled in base currency)
    pub fn new_inverse(underlying: impl Into<String>, symbol: impl Into<String>) -> Self {
        Self {
            underlying: underlying.into(),
            symbol: symbol.into(),
            tick_size: dec!(0.5),
            lot_size: dec!(1), // Contract quantity
            multiplier: dec!(1),
            initial_margin: dec!(0.01),
            funding_interval_hours: 8,
            is_inverse: true,
        }
    }

    /// Builder methods
    pub fn with_tick_size(mut self, tick: Price) -> Self {
        self.tick_size = tick;
        self
    }

    pub fn with_lot_size(mut self, lot: Quantity) -> Self {
        self.lot_size = lot;
        self
    }

    pub fn with_multiplier(mut self, mult: Decimal) -> Self {
        self.multiplier = mult;
        self
    }

    pub fn with_margin(mut self, margin: Decimal) -> Self {
        self.initial_margin = margin;
        self
    }

    pub fn with_funding_interval(mut self, hours: u32) -> Self {
        self.funding_interval_hours = hours;
        self
    }

    /// Calculate position value for linear perp
    pub fn position_value_linear(&self, price: Price, quantity: Quantity) -> Decimal {
        price * quantity * self.multiplier
    }

    /// Calculate position value for inverse perp
    /// For inverse, the formula is: quantity * multiplier / price
    pub fn position_value_inverse(&self, price: Price, quantity: Quantity) -> Decimal {
        if price == Decimal::ZERO {
            return Decimal::ZERO;
        }
        quantity * self.multiplier / price
    }

    /// Calculate position value based on contract type
    pub fn position_value(&self, price: Price, quantity: Quantity) -> Decimal {
        if self.is_inverse {
            self.position_value_inverse(price, quantity)
        } else {
            self.position_value_linear(price, quantity)
        }
    }

    /// Common perpetual contracts
    pub fn btc_perp() -> Self {
        Self::new_linear("BTC", "BTC-PERP")
            .with_tick_size(dec!(0.1))
            .with_lot_size(dec!(0.001))
    }

    pub fn eth_perp() -> Self {
        Self::new_linear("ETH", "ETH-PERP")
            .with_tick_size(dec!(0.01))
            .with_lot_size(dec!(0.01))
    }
}

impl InstrumentSpec for PerpetualContract {
    fn symbol(&self) -> &str {
        &self.symbol
    }

    fn tick_size(&self) -> Price {
        self.tick_size
    }

    fn lot_size(&self) -> Quantity {
        self.lot_size
    }

    fn margin_requirement(&self) -> Decimal {
        self.initial_margin
    }

    fn is_shortable(&self) -> bool {
        true
    }
}

impl std::fmt::Display for PerpetualContract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_perp() {
        let perp = PerpetualContract::btc_perp();
        assert_eq!(perp.underlying, "BTC");
        assert_eq!(perp.symbol, "BTC-PERP");
        assert!(!perp.is_inverse);
        assert_eq!(perp.max_leverage(), dec!(100)); // 1 / 0.01 = 100
    }

    #[test]
    fn test_inverse_perp() {
        let perp = PerpetualContract::new_inverse("BTC", "BTCUSD");
        assert!(perp.is_inverse);
    }

    #[test]
    fn test_linear_position_value() {
        let perp = PerpetualContract::new_linear("BTC", "BTC-PERP");
        let value = perp.position_value_linear(dec!(50000), dec!(0.1));
        assert_eq!(value, dec!(5000));
    }

    #[test]
    fn test_inverse_position_value() {
        let perp = PerpetualContract::new_inverse("BTC", "BTCUSD").with_multiplier(dec!(100));
        // 1 contract at $50000 = 100 / 50000 = 0.002 BTC
        let value = perp.position_value_inverse(dec!(50000), dec!(1));
        assert_eq!(value, dec!(0.002));
    }
}
