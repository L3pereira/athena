use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::values::{Price, Quantity, Timestamp};

/// A futures contract (e.g., BTC-DEC24)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FutureContract {
    /// Underlying asset
    pub underlying: String,
    /// Contract expiration date
    pub expiry: Timestamp,
    /// Contract symbol (e.g., "BTC-DEC24")
    pub symbol: String,
    /// Minimum price increment
    pub tick_size: Price,
    /// Minimum quantity increment (contract size)
    pub lot_size: Quantity,
    /// Contract multiplier (notional value per contract)
    pub multiplier: Decimal,
    /// Initial margin requirement (as decimal, e.g., 0.05 = 5%)
    pub initial_margin: Decimal,
    /// Settlement type
    pub settlement: SettlementType,
}

/// How the future settles at expiry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SettlementType {
    /// Physical delivery of underlying
    Physical,
    /// Cash settlement based on index price
    Cash,
}

impl FutureContract {
    /// Create a new futures contract
    pub fn new(
        underlying: impl Into<String>,
        expiry: Timestamp,
        symbol: impl Into<String>,
    ) -> Self {
        Self {
            underlying: underlying.into(),
            expiry,
            symbol: symbol.into(),
            tick_size: dec!(0.01),
            lot_size: dec!(1), // 1 contract minimum
            multiplier: dec!(1),
            initial_margin: dec!(0.05), // 5% = 20x leverage
            settlement: SettlementType::Cash,
        }
    }

    /// Builder pattern for customization
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

    pub fn with_settlement(mut self, settlement: SettlementType) -> Self {
        self.settlement = settlement;
        self
    }

    /// Check if the contract has expired
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expiry
    }

    /// Time remaining until expiry
    pub fn time_to_expiry(&self, now: Timestamp) -> chrono::Duration {
        self.expiry - now
    }

    /// Calculate notional value
    pub fn notional_value(&self, price: Price, quantity: Quantity) -> Decimal {
        price * quantity * self.multiplier
    }
}

impl InstrumentSpec for FutureContract {
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
        true // Futures are always shortable
    }
}

impl std::fmt::Display for FutureContract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_expiry() -> Timestamp {
        Utc.with_ymd_and_hms(2024, 12, 27, 8, 0, 0).unwrap()
    }

    #[test]
    fn test_future_creation() {
        let expiry = make_expiry();
        let future = FutureContract::new("BTC", expiry, "BTC-DEC24");

        assert_eq!(future.underlying, "BTC");
        assert_eq!(future.symbol, "BTC-DEC24");
        assert_eq!(future.initial_margin, dec!(0.05));
    }

    #[test]
    fn test_future_expiry() {
        let expiry = make_expiry();
        let future = FutureContract::new("BTC", expiry, "BTC-DEC24");

        let before = Utc.with_ymd_and_hms(2024, 12, 26, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2024, 12, 28, 0, 0, 0).unwrap();

        assert!(!future.is_expired(before));
        assert!(future.is_expired(after));
        assert!(future.is_expired(expiry));
    }

    #[test]
    fn test_future_leverage() {
        let expiry = make_expiry();
        let future = FutureContract::new("BTC", expiry, "BTC-DEC24");

        assert_eq!(future.max_leverage(), dec!(20)); // 1 / 0.05 = 20
    }

    #[test]
    fn test_notional_value() {
        let expiry = make_expiry();
        let future = FutureContract::new("BTC", expiry, "BTC-DEC24").with_multiplier(dec!(10));

        let notional = future.notional_value(dec!(50000), dec!(5));
        assert_eq!(notional, dec!(2500000)); // 50000 * 5 * 10
    }
}
