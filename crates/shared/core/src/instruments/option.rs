use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::values::{Price, Quantity, Timestamp};

/// Option type: Call (right to buy) or Put (right to sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionType {
    Call,
    Put,
}

impl std::fmt::Display for OptionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptionType::Call => write!(f, "C"),
            OptionType::Put => write!(f, "P"),
        }
    }
}

/// Option exercise style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExerciseStyle {
    /// Can only exercise at expiry
    European,
    /// Can exercise any time before expiry
    American,
}

/// An option contract (e.g., BTC-DEC24-50000-C)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OptionContract {
    /// Underlying asset
    pub underlying: String,
    /// Expiration timestamp
    pub expiry: Timestamp,
    /// Strike price
    pub strike: Price,
    /// Option type (Call or Put)
    pub option_type: OptionType,
    /// Contract symbol
    pub symbol: String,
    /// Minimum price increment (for premium)
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
    /// Contract multiplier (shares per contract)
    pub multiplier: Decimal,
    /// Exercise style
    pub exercise_style: ExerciseStyle,
    /// Initial margin requirement for sellers
    pub seller_margin: Decimal,
}

impl OptionContract {
    /// Create a new option contract
    pub fn new(
        underlying: impl Into<String>,
        expiry: Timestamp,
        strike: Price,
        option_type: OptionType,
    ) -> Self {
        let underlying = underlying.into();
        let symbol = format!(
            "{}-{}-{}-{}",
            underlying,
            expiry.format("%b%y").to_string().to_uppercase(),
            strike,
            option_type
        );

        Self {
            underlying,
            expiry,
            strike,
            option_type,
            symbol,
            tick_size: dec!(0.01),
            lot_size: dec!(0.01),
            multiplier: dec!(1),
            exercise_style: ExerciseStyle::European,
            seller_margin: dec!(0.15), // 15% for option sellers
        }
    }

    /// Create with custom symbol
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = symbol.into();
        self
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

    pub fn with_exercise_style(mut self, style: ExerciseStyle) -> Self {
        self.exercise_style = style;
        self
    }

    pub fn with_seller_margin(mut self, margin: Decimal) -> Self {
        self.seller_margin = margin;
        self
    }

    /// Check if option has expired
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expiry
    }

    /// Time remaining until expiry
    pub fn time_to_expiry(&self, now: Timestamp) -> chrono::Duration {
        self.expiry - now
    }

    /// Calculate intrinsic value
    pub fn intrinsic_value(&self, spot_price: Price) -> Decimal {
        match self.option_type {
            OptionType::Call => (spot_price - self.strike).max(Decimal::ZERO),
            OptionType::Put => (self.strike - spot_price).max(Decimal::ZERO),
        }
    }

    /// Check if option is in the money
    pub fn is_in_the_money(&self, spot_price: Price) -> bool {
        self.intrinsic_value(spot_price) > Decimal::ZERO
    }

    /// Check if option is at the money (within 0.5% of strike)
    pub fn is_at_the_money(&self, spot_price: Price) -> bool {
        let diff = ((spot_price - self.strike) / self.strike).abs();
        diff <= dec!(0.005)
    }

    /// Check if option is out of the money
    pub fn is_out_of_the_money(&self, spot_price: Price) -> bool {
        !self.is_in_the_money(spot_price) && !self.is_at_the_money(spot_price)
    }

    /// Calculate notional exposure
    pub fn notional_value(&self, quantity: Quantity) -> Decimal {
        self.strike * quantity * self.multiplier
    }
}

impl InstrumentSpec for OptionContract {
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
        // Buyers: premium paid upfront (handled separately)
        // Sellers: need margin
        self.seller_margin
    }

    fn is_shortable(&self) -> bool {
        true // Can sell (write) options
    }
}

impl std::fmt::Display for OptionContract {
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
    fn test_option_creation() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, dec!(50000), OptionType::Call);

        assert_eq!(call.underlying, "BTC");
        assert_eq!(call.strike, dec!(50000));
        assert_eq!(call.option_type, OptionType::Call);
        assert!(call.symbol.contains("BTC"));
        assert!(call.symbol.contains("50000"));
        assert!(call.symbol.contains("C"));
    }

    #[test]
    fn test_call_intrinsic_value() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, dec!(50000), OptionType::Call);

        // ITM call
        assert_eq!(call.intrinsic_value(dec!(55000)), dec!(5000));
        // ATM call
        assert_eq!(call.intrinsic_value(dec!(50000)), dec!(0));
        // OTM call
        assert_eq!(call.intrinsic_value(dec!(45000)), dec!(0));
    }

    #[test]
    fn test_put_intrinsic_value() {
        let expiry = make_expiry();
        let put = OptionContract::new("BTC", expiry, dec!(50000), OptionType::Put);

        // OTM put
        assert_eq!(put.intrinsic_value(dec!(55000)), dec!(0));
        // ATM put
        assert_eq!(put.intrinsic_value(dec!(50000)), dec!(0));
        // ITM put
        assert_eq!(put.intrinsic_value(dec!(45000)), dec!(5000));
    }

    #[test]
    fn test_moneyness() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, dec!(50000), OptionType::Call);

        assert!(call.is_in_the_money(dec!(55000)));
        assert!(call.is_at_the_money(dec!(50000)));
        assert!(call.is_out_of_the_money(dec!(45000)));
    }

    #[test]
    fn test_option_expiry() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, dec!(50000), OptionType::Call);

        let before = Utc.with_ymd_and_hms(2024, 12, 26, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2024, 12, 28, 0, 0, 0).unwrap();

        assert!(!call.is_expired(before));
        assert!(call.is_expired(after));
    }
}
