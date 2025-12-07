use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::domain::{Price, Quantity};

/// A perpetual swap contract (e.g., BTC-PERP)
///
/// Perpetuals are like futures but without expiry. They use a funding rate
/// mechanism to keep the contract price aligned with the underlying spot price.
///
/// Two types:
/// - Linear: Settled in quote currency (USDT), PnL = qty * (exit - entry)
/// - Inverse: Settled in base currency (BTC), PnL = qty * (1/entry - 1/exit)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PerpetualContract {
    /// Underlying asset (e.g., BTC)
    pub underlying: String,
    /// Contract symbol (e.g., BTC-PERP)
    pub symbol: String,
    /// Minimum price increment
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
    /// Contract multiplier (notional per contract)
    pub multiplier: Decimal,
    /// Initial margin requirement
    pub initial_margin: Decimal,
    /// Maintenance margin requirement
    pub maintenance_margin: Decimal,
    /// Funding interval in hours (typically 8)
    pub funding_interval_hours: u32,
    /// Whether this is an inverse contract
    pub is_inverse: bool,
}

impl PerpetualContract {
    /// Create a new linear perpetual (settled in quote currency)
    pub fn linear(underlying: impl Into<String>, symbol: impl Into<String>) -> Self {
        Self {
            underlying: underlying.into(),
            symbol: symbol.into(),
            tick_size: Price::from(dec!(0.01)),
            lot_size: Quantity::from(dec!(0.001)),
            multiplier: dec!(1),
            initial_margin: dec!(0.01),      // 1% = 100x max leverage
            maintenance_margin: dec!(0.005), // 0.5%
            funding_interval_hours: 8,
            is_inverse: false,
        }
    }

    /// Create a new inverse perpetual (settled in base currency)
    pub fn inverse(underlying: impl Into<String>, symbol: impl Into<String>) -> Self {
        Self {
            underlying: underlying.into(),
            symbol: symbol.into(),
            tick_size: Price::from(dec!(0.5)),
            lot_size: Quantity::from(dec!(1)), // Contract quantity
            multiplier: dec!(1),               // USD per contract
            initial_margin: dec!(0.01),
            maintenance_margin: dec!(0.005),
            funding_interval_hours: 8,
            is_inverse: true,
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

    pub fn with_multiplier(mut self, mult: Decimal) -> Self {
        self.multiplier = mult;
        self
    }

    pub fn with_initial_margin(mut self, margin: Decimal) -> Self {
        self.initial_margin = margin;
        self
    }

    pub fn with_maintenance_margin(mut self, margin: Decimal) -> Self {
        self.maintenance_margin = margin;
        self
    }

    pub fn with_funding_interval(mut self, hours: u32) -> Self {
        self.funding_interval_hours = hours;
        self
    }

    /// Calculate position value for linear perp
    /// Value = price * quantity * multiplier
    pub fn position_value_linear(&self, price: Price, quantity: Quantity) -> Decimal {
        price.inner() * quantity.inner() * self.multiplier
    }

    /// Calculate position value for inverse perp
    /// Value = quantity * multiplier / price (in base currency)
    pub fn position_value_inverse(&self, price: Price, quantity: Quantity) -> Decimal {
        if price.is_zero() {
            return Decimal::ZERO;
        }
        quantity.inner() * self.multiplier / price.inner()
    }

    /// Calculate position value based on contract type
    pub fn position_value(&self, price: Price, quantity: Quantity) -> Decimal {
        if self.is_inverse {
            self.position_value_inverse(price, quantity)
        } else {
            self.position_value_linear(price, quantity)
        }
    }

    /// Calculate PnL for a position
    pub fn calculate_pnl(
        &self,
        entry_price: Price,
        exit_price: Price,
        quantity: Quantity,
        is_long: bool,
    ) -> Decimal {
        let direction = if is_long { dec!(1) } else { dec!(-1) };

        if self.is_inverse {
            // Inverse PnL = qty * mult * (1/entry - 1/exit) for long
            // When price goes UP (entry < exit), 1/entry > 1/exit, so (1/entry - 1/exit) > 0
            let entry_inv = Decimal::ONE / entry_price.inner();
            let exit_inv = Decimal::ONE / exit_price.inner();
            direction * quantity.inner() * self.multiplier * (entry_inv - exit_inv)
        } else {
            // Linear PnL = qty * mult * (exit - entry) for long
            direction
                * quantity.inner()
                * self.multiplier
                * (exit_price.inner() - entry_price.inner())
        }
    }

    /// Common perpetuals
    pub fn btc_perp() -> Self {
        Self::linear("BTC", "BTC-PERP")
            .with_tick_size(Price::from(dec!(0.1)))
            .with_lot_size(Quantity::from(dec!(0.001)))
    }

    pub fn eth_perp() -> Self {
        Self::linear("ETH", "ETH-PERP")
            .with_tick_size(Price::from(dec!(0.01)))
            .with_lot_size(Quantity::from(dec!(0.01)))
    }

    pub fn btcusd_inverse() -> Self {
        Self::inverse("BTC", "BTCUSD")
            .with_multiplier(dec!(100)) // $100 per contract
            .with_tick_size(Price::from(dec!(0.5)))
            .with_lot_size(Quantity::from(dec!(1)))
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

    fn base_asset(&self) -> &str {
        &self.underlying
    }

    fn quote_asset(&self) -> &str {
        if self.is_inverse {
            &self.underlying
        } else {
            "USDT"
        }
    }

    fn is_derivative(&self) -> bool {
        true
    }

    fn margin_requirement(&self) -> Decimal {
        self.initial_margin
    }

    fn is_shortable(&self) -> bool {
        true // Perpetuals can always be shorted
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
        assert!(!perp.is_inverse);
        assert_eq!(perp.max_leverage(), dec!(100));
    }

    #[test]
    fn test_inverse_perp() {
        let perp = PerpetualContract::btcusd_inverse();
        assert!(perp.is_inverse);
        assert_eq!(perp.multiplier, dec!(100));
    }

    #[test]
    fn test_linear_pnl() {
        let perp = PerpetualContract::linear("BTC", "BTC-PERP");

        // Long 1 BTC, entry 50000, exit 51000 = +1000 profit
        let pnl = perp.calculate_pnl(
            Price::from(dec!(50000)),
            Price::from(dec!(51000)),
            Quantity::from(dec!(1)),
            true,
        );
        assert_eq!(pnl, dec!(1000));

        // Short 1 BTC, entry 50000, exit 51000 = -1000 loss
        let pnl = perp.calculate_pnl(
            Price::from(dec!(50000)),
            Price::from(dec!(51000)),
            Quantity::from(dec!(1)),
            false,
        );
        assert_eq!(pnl, dec!(-1000));
    }

    #[test]
    fn test_inverse_pnl() {
        let perp = PerpetualContract::inverse("BTC", "BTCUSD").with_multiplier(dec!(100));

        // Long 100 contracts ($100 each), entry 50000, exit 51000
        // PnL = 100 * 100 * (1/50000 - 1/51000) = 0.0392... BTC
        let pnl = perp.calculate_pnl(
            Price::from(dec!(50000)),
            Price::from(dec!(51000)),
            Quantity::from(dec!(100)),
            true,
        );
        assert!(pnl > Decimal::ZERO);
    }

    #[test]
    fn test_position_value() {
        let linear = PerpetualContract::btc_perp();
        let value = linear.position_value(Price::from(dec!(50000)), Quantity::from(dec!(1)));
        assert_eq!(value, dec!(50000));

        let inverse = PerpetualContract::btcusd_inverse();
        // 100 contracts at $50000 = 100 * 100 / 50000 = 0.2 BTC
        let value = inverse.position_value(Price::from(dec!(50000)), Quantity::from(dec!(100)));
        assert_eq!(value, dec!(0.2));
    }
}
