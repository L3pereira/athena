use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::domain::{PRICE_SCALE, Price, Quantity, Rate, Timestamp, Value};

/// Option type: Call (right to buy) or Put (right to sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
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
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
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
    /// Contract multiplier (scaled by PRICE_SCALE)
    pub multiplier: i64,
    /// Exercise style
    pub exercise_style: ExerciseStyle,
    /// Margin requirement for sellers (writers) in basis points (e.g., 1500 = 15%)
    pub seller_margin_bps: i64,
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
            tick_size: Price::from_f64(0.0001), // Options often quoted in small increments
            lot_size: Quantity::from_f64(0.01),
            multiplier: PRICE_SCALE, // 1.0 scaled
            exercise_style: ExerciseStyle::European,
            seller_margin_bps: 1500, // 15% for option sellers
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

    pub fn with_multiplier(mut self, mult: i64) -> Self {
        self.multiplier = mult;
        self
    }

    pub fn with_exercise_style(mut self, style: ExerciseStyle) -> Self {
        self.exercise_style = style;
        self
    }

    /// Set seller margin in basis points (e.g., 1500 = 15%)
    pub fn with_seller_margin_bps(mut self, margin_bps: i64) -> Self {
        self.seller_margin_bps = margin_bps;
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
    pub fn intrinsic_value(&self, spot_price: Price) -> Value {
        match self.option_type {
            OptionType::Call => {
                let diff = spot_price.raw() - self.strike.raw();
                if diff > 0 {
                    Value::from_raw(diff as i128)
                } else {
                    Value::ZERO
                }
            }
            OptionType::Put => {
                let diff = self.strike.raw() - spot_price.raw();
                if diff > 0 {
                    Value::from_raw(diff as i128)
                } else {
                    Value::ZERO
                }
            }
        }
    }

    /// Check if option is in the money
    pub fn is_in_the_money(&self, spot_price: Price) -> bool {
        !self.intrinsic_value(spot_price).is_zero()
    }

    /// Check if option is at the money (within 0.5% of strike)
    pub fn is_at_the_money(&self, spot_price: Price) -> bool {
        if self.strike.is_zero() {
            return false;
        }
        let diff = (spot_price.raw() - self.strike.raw()).abs();
        // 0.5% = 50 bps, check if diff / strike < 0.005
        diff * 200 < self.strike.raw()
    }

    /// Check if option is out of the money
    pub fn is_out_of_the_money(&self, spot_price: Price) -> bool {
        !self.is_in_the_money(spot_price) && !self.is_at_the_money(spot_price)
    }

    /// Moneyness as basis points (10000 = 1.0 = at the money)
    /// For calls: spot / strike * 10000, for puts: strike / spot * 10000
    pub fn moneyness_bps(&self, spot_price: Price) -> i64 {
        if self.strike.is_zero() || spot_price.is_zero() {
            return 10000; // 1.0 at the money
        }
        match self.option_type {
            OptionType::Call => spot_price.raw() * 10000 / self.strike.raw(),
            OptionType::Put => self.strike.raw() * 10000 / spot_price.raw(),
        }
    }

    /// Calculate notional exposure
    pub fn notional_value(&self, quantity: Quantity) -> Value {
        // strike * qty * multiplier / PRICE_SCALE
        let base = self.strike.mul_qty(quantity);
        Value::from_raw(base.raw() * self.multiplier as i128 / PRICE_SCALE as i128)
    }

    /// Delta approximation in basis points (simplified, not Black-Scholes)
    /// Returns rough delta * 10000 based on moneyness
    pub fn approximate_delta_bps(&self, spot_price: Price) -> i64 {
        let m = self.moneyness_bps(spot_price);
        match self.option_type {
            OptionType::Call => {
                if m < 8000 {
                    1000 // 0.1
                } else if m < 9500 {
                    3000 // 0.3
                } else if m < 10500 {
                    5000 // 0.5
                } else if m < 12000 {
                    7000 // 0.7
                } else {
                    9000 // 0.9
                }
            }
            OptionType::Put => {
                if m < 8000 {
                    -9000 // -0.9
                } else if m < 9500 {
                    -7000 // -0.7
                } else if m < 10500 {
                    -5000 // -0.5
                } else if m < 12000 {
                    -3000 // -0.3
                } else {
                    -1000 // -0.1
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

    fn margin_requirement(&self) -> Rate {
        // Buyers pay premium upfront (no margin)
        // Sellers need margin
        Rate::from_bps(self.seller_margin_bps)
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
        let call = OptionContract::new("BTC", expiry, Price::from_int(50000), OptionType::Call);

        assert_eq!(call.underlying, "BTC");
        assert_eq!(call.strike, Price::from_int(50000));
        assert_eq!(call.option_type, OptionType::Call);
        assert!(call.symbol.contains("BTC"));
        assert!(call.symbol.contains("50000"));
    }

    #[test]
    fn test_call_intrinsic_value() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, Price::from_int(50000), OptionType::Call);

        // ITM: spot 55000, strike 50000 -> intrinsic = 5000
        assert_eq!(
            call.intrinsic_value(Price::from_int(55000)),
            Value::from_raw(5000 * PRICE_SCALE as i128)
        );
        // ATM: spot = strike -> intrinsic = 0
        assert_eq!(call.intrinsic_value(Price::from_int(50000)), Value::ZERO);
        // OTM: spot 45000 < strike -> intrinsic = 0
        assert_eq!(call.intrinsic_value(Price::from_int(45000)), Value::ZERO);
    }

    #[test]
    fn test_put_intrinsic_value() {
        let expiry = make_expiry();
        let put = OptionContract::new("BTC", expiry, Price::from_int(50000), OptionType::Put);

        // OTM: spot 55000 > strike -> intrinsic = 0
        assert_eq!(put.intrinsic_value(Price::from_int(55000)), Value::ZERO);
        // ATM: spot = strike -> intrinsic = 0
        assert_eq!(put.intrinsic_value(Price::from_int(50000)), Value::ZERO);
        // ITM: spot 45000, strike 50000 -> intrinsic = 5000
        assert_eq!(
            put.intrinsic_value(Price::from_int(45000)),
            Value::from_raw(5000 * PRICE_SCALE as i128)
        );
    }

    #[test]
    fn test_moneyness() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, Price::from_int(50000), OptionType::Call);

        assert!(call.is_in_the_money(Price::from_int(55000)));
        assert!(call.is_at_the_money(Price::from_int(50000)));
        assert!(call.is_out_of_the_money(Price::from_int(45000)));
    }

    #[test]
    fn test_expiry() {
        let expiry = make_expiry();
        let call = OptionContract::new("BTC", expiry, Price::from_int(50000), OptionType::Call);

        let before = Utc.with_ymd_and_hms(2024, 12, 26, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2024, 12, 28, 0, 0, 0).unwrap();

        assert!(!call.is_expired(before));
        assert!(call.is_expired(after));
    }
}
