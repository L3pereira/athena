use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::domain::{Price, Quantity, Timestamp};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ExerciseStyle {
    /// Can only exercise at expiry (most crypto options)
    #[default]
    European,
    /// Can exercise any time before expiry
    American,
}

/// An option contract (e.g., BTC-DEC24-50000-C)
///
/// Options give the holder the right (not obligation) to buy or sell
/// the underlying at the strike price.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OptionContract {
    /// Underlying asset (e.g., BTC)
    pub underlying: String,
    /// Contract symbol (e.g., BTC-27DEC24-50000-C)
    pub symbol: String,
    /// Expiration timestamp
    pub expiry: Timestamp,
    /// Strike price
    pub strike: Price,
    /// Option type (Call or Put)
    pub option_type: OptionType,
    /// Minimum price increment (for premium)
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
    /// Contract multiplier
    pub multiplier: Decimal,
    /// Exercise style
    pub exercise_style: ExerciseStyle,
    /// Margin requirement for sellers (writers)
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
            expiry.format("%d%b%y").to_string().to_uppercase(),
            strike,
            option_type
        );

        Self {
            underlying,
            symbol,
            expiry,
            strike,
            option_type,
            tick_size: Price::from(dec!(0.0001)), // Options often quoted in small increments
            lot_size: Quantity::from(dec!(0.01)),
            multiplier: dec!(1),
            exercise_style: ExerciseStyle::European,
            seller_margin: dec!(0.15), // 15% for option sellers
        }
    }

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

    /// Years to expiry (for Black-Scholes)
    pub fn years_to_expiry(&self, now: Timestamp) -> f64 {
        let duration = self.time_to_expiry(now);
        duration.num_milliseconds() as f64 / (365.25 * 24.0 * 60.0 * 60.0 * 1000.0)
    }

    /// Calculate intrinsic value (value if exercised now)
    pub fn intrinsic_value(&self, spot_price: Price) -> Decimal {
        match self.option_type {
            OptionType::Call => (spot_price.inner() - self.strike.inner()).max(Decimal::ZERO),
            OptionType::Put => (self.strike.inner() - spot_price.inner()).max(Decimal::ZERO),
        }
    }

    /// Check if option is in the money
    pub fn is_in_the_money(&self, spot_price: Price) -> bool {
        self.intrinsic_value(spot_price) > Decimal::ZERO
    }

    /// Check if option is at the money (within 0.5% of strike)
    pub fn is_at_the_money(&self, spot_price: Price) -> bool {
        if self.strike.is_zero() {
            return false;
        }
        let diff = ((spot_price.inner() - self.strike.inner()) / self.strike.inner()).abs();
        diff <= dec!(0.005)
    }

    /// Check if option is out of the money
    pub fn is_out_of_the_money(&self, spot_price: Price) -> bool {
        !self.is_in_the_money(spot_price) && !self.is_at_the_money(spot_price)
    }

    /// Moneyness: spot / strike (for calls), strike / spot (for puts)
    pub fn moneyness(&self, spot_price: Price) -> Decimal {
        if self.strike.is_zero() || spot_price.is_zero() {
            return Decimal::ONE;
        }
        match self.option_type {
            OptionType::Call => spot_price.inner() / self.strike.inner(),
            OptionType::Put => self.strike.inner() / spot_price.inner(),
        }
    }

    /// Calculate notional exposure
    pub fn notional_value(&self, quantity: Quantity) -> Decimal {
        self.strike.inner() * quantity.inner() * self.multiplier
    }

    /// Delta approximation (simplified, not Black-Scholes)
    /// Returns rough delta based on moneyness
    pub fn approximate_delta(&self, spot_price: Price) -> Decimal {
        let m = self.moneyness(spot_price);
        match self.option_type {
            OptionType::Call => {
                if m < dec!(0.8) {
                    dec!(0.1)
                } else if m < dec!(0.95) {
                    dec!(0.3)
                } else if m < dec!(1.05) {
                    dec!(0.5)
                } else if m < dec!(1.2) {
                    dec!(0.7)
                } else {
                    dec!(0.9)
                }
            }
            OptionType::Put => {
                if m < dec!(0.8) {
                    dec!(-0.9)
                } else if m < dec!(0.95) {
                    dec!(-0.7)
                } else if m < dec!(1.05) {
                    dec!(-0.5)
                } else if m < dec!(1.2) {
                    dec!(-0.3)
                } else {
                    dec!(-0.1)
                }
            }
        }
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

    fn base_asset(&self) -> &str {
        &self.underlying
    }

    fn quote_asset(&self) -> &str {
        "USDT" // Options typically settle in USDT
    }

    fn is_derivative(&self) -> bool {
        true
    }

    fn margin_requirement(&self) -> Decimal {
        // Buyers pay premium upfront (no margin)
        // Sellers need margin
        self.seller_margin
    }

    fn is_shortable(&self) -> bool {
        true // Can write (sell) options
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
        let call = OptionContract::new("BTC", expiry, Price::from(dec!(50000)), OptionType::Call);

        assert_eq!(call.underlying, "BTC");
        assert_eq!(call.strike, Price::from(dec!(50000)));
        assert_eq!(call.option_type, OptionType::Call);
        assert!(call.symbol.contains("BTC"));
        assert!(call.symbol.contains("50000"));
    }

    #[test]
    fn test_call_intrinsic_value() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, Price::from(dec!(50000)), OptionType::Call);

        // ITM: spot 55000, strike 50000 -> intrinsic = 5000
        assert_eq!(call.intrinsic_value(Price::from(dec!(55000))), dec!(5000));
        // ATM: spot = strike -> intrinsic = 0
        assert_eq!(call.intrinsic_value(Price::from(dec!(50000))), dec!(0));
        // OTM: spot 45000 < strike -> intrinsic = 0
        assert_eq!(call.intrinsic_value(Price::from(dec!(45000))), dec!(0));
    }

    #[test]
    fn test_put_intrinsic_value() {
        let expiry = make_expiry();
        let put = OptionContract::new("BTC", expiry, Price::from(dec!(50000)), OptionType::Put);

        // OTM: spot 55000 > strike -> intrinsic = 0
        assert_eq!(put.intrinsic_value(Price::from(dec!(55000))), dec!(0));
        // ATM: spot = strike -> intrinsic = 0
        assert_eq!(put.intrinsic_value(Price::from(dec!(50000))), dec!(0));
        // ITM: spot 45000, strike 50000 -> intrinsic = 5000
        assert_eq!(put.intrinsic_value(Price::from(dec!(45000))), dec!(5000));
    }

    #[test]
    fn test_moneyness() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, Price::from(dec!(50000)), OptionType::Call);

        assert!(call.is_in_the_money(Price::from(dec!(55000))));
        assert!(call.is_at_the_money(Price::from(dec!(50000))));
        assert!(call.is_out_of_the_money(Price::from(dec!(45000))));
    }

    #[test]
    fn test_expiry() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, Price::from(dec!(50000)), OptionType::Call);

        let before = Utc.with_ymd_and_hms(2024, 12, 26, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2024, 12, 28, 0, 0, 0).unwrap();

        assert!(!call.is_expired(before));
        assert!(call.is_expired(after));
    }
}
