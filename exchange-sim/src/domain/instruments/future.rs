use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::domain::{PRICE_SCALE, Price, Quantity, Rate, Timestamp, Value};

/// A futures contract (e.g., BTC-DEC24)
///
/// Futures have a fixed expiration date and settle at that date.
/// They can be either physically settled (delivery) or cash settled.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FutureContract {
    /// Underlying asset (e.g., BTC)
    pub underlying: String,
    /// Contract symbol (e.g., BTC-DEC24)
    pub symbol: String,
    /// Expiration date/time
    pub expiry: Timestamp,
    /// Minimum price increment
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
    /// Contract multiplier (scaled by PRICE_SCALE)
    pub multiplier: i64,
    /// Initial margin requirement in basis points (e.g., 200 = 2% = 50x max leverage)
    pub initial_margin_bps: i64,
    /// Settlement type
    pub settlement: SettlementType,
    /// Whether this is an inverse contract
    pub is_inverse: bool,
}

/// How the future settles at expiry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SettlementType {
    /// Physical delivery of the underlying
    Physical,
    /// Cash settlement based on index price
    Cash,
}

impl FutureContract {
    /// Create a new linear future (settled in quote currency)
    pub fn linear(
        underlying: impl Into<String>,
        symbol: impl Into<String>,
        expiry: Timestamp,
    ) -> Self {
        Self {
            underlying: underlying.into(),
            symbol: symbol.into(),
            expiry,
            tick_size: Price::from_f64(0.01),
            lot_size: Quantity::from_f64(0.001),
            multiplier: PRICE_SCALE, // 1.0 scaled
            initial_margin_bps: 200, // 2% = 50x max leverage
            settlement: SettlementType::Cash,
            is_inverse: false,
        }
    }

    /// Create a new inverse future
    pub fn inverse(
        underlying: impl Into<String>,
        symbol: impl Into<String>,
        expiry: Timestamp,
    ) -> Self {
        Self {
            underlying: underlying.into(),
            symbol: symbol.into(),
            expiry,
            tick_size: Price::from_f64(0.5),
            lot_size: Quantity::from_f64(1.0),
            multiplier: PRICE_SCALE, // 1.0 scaled
            initial_margin_bps: 200, // 2%
            settlement: SettlementType::Cash,
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

    pub fn with_multiplier(mut self, mult: i64) -> Self {
        self.multiplier = mult;
        self
    }

    /// Set initial margin in basis points (e.g., 200 = 2%)
    pub fn with_initial_margin_bps(mut self, margin_bps: i64) -> Self {
        self.initial_margin_bps = margin_bps;
        self
    }

    pub fn with_settlement(mut self, settlement: SettlementType) -> Self {
        self.settlement = settlement;
        self
    }

    /// Check if the future has expired
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expiry
    }

    /// Time remaining until expiry
    pub fn time_to_expiry(&self, now: Timestamp) -> chrono::Duration {
        self.expiry - now
    }

    /// Days to expiry (fractional)
    pub fn days_to_expiry(&self, now: Timestamp) -> f64 {
        let duration = self.time_to_expiry(now);
        duration.num_milliseconds() as f64 / (24.0 * 60.0 * 60.0 * 1000.0)
    }

    /// Calculate annualized basis (future premium/discount) in basis points
    /// Basis = (future_price - spot_price) / spot_price * (365 / days_to_expiry)
    pub fn annualized_basis_bps(
        &self,
        future_price: Price,
        spot_price: Price,
        now: Timestamp,
    ) -> i64 {
        if spot_price.is_zero() {
            return 0;
        }

        let days = self.days_to_expiry(now);
        if days <= 0.0 {
            return 0;
        }

        // Calculate basis as (future - spot) / spot * 365 / days * 10000 (for bps)
        let price_diff = future_price.raw() - spot_price.raw();
        let basis_pct = (price_diff as f64 / spot_price.raw() as f64) * (365.0 / days);
        (basis_pct * 10000.0) as i64
    }

    /// Calculate position value
    pub fn position_value(&self, price: Price, quantity: Quantity) -> Value {
        if self.is_inverse {
            if price.is_zero() {
                return Value::ZERO;
            }
            // For inverse: qty * multiplier / price (settled in base asset)
            // multiplier is scaled by PRICE_SCALE, so: qty.raw() * mult / price.raw()
            let qty_scaled = quantity.raw() as i128 * self.multiplier as i128;
            Value::from_raw(qty_scaled / price.raw() as i128)
        } else {
            // For linear: price * qty * multiplier / PRICE_SCALE (multiplier is scaled)
            let notional = price.mul_qty(quantity);
            Value::from_raw(notional.raw() * self.multiplier as i128 / PRICE_SCALE as i128)
        }
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

    fn margin_requirement(&self) -> Rate {
        Rate::from_bps(self.initial_margin_bps)
    }

    fn is_shortable(&self) -> bool {
        true
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
        let future = FutureContract::linear("BTC", "BTC-DEC24", expiry);

        assert_eq!(future.underlying, "BTC");
        assert_eq!(future.symbol, "BTC-DEC24");
        assert!(!future.is_inverse);
    }

    #[test]
    fn test_expiry() {
        let expiry = make_expiry();
        let future = FutureContract::linear("BTC", "BTC-DEC24", expiry);

        let before = Utc.with_ymd_and_hms(2024, 12, 26, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2024, 12, 28, 0, 0, 0).unwrap();

        assert!(!future.is_expired(before));
        assert!(future.is_expired(after));
    }

    #[test]
    fn test_annualized_basis() {
        let expiry = Utc.with_ymd_and_hms(2025, 3, 28, 8, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2024, 12, 28, 8, 0, 0).unwrap();
        let future = FutureContract::linear("BTC", "BTC-MAR25", expiry);

        // 90 days to expiry, future at 51000, spot at 50000
        // Basis = (51000-50000)/50000 * (365/90) = 0.02 * 4.05 = ~8.1% = ~810 bps
        let basis_bps =
            future.annualized_basis_bps(Price::from_int(51000), Price::from_int(50000), now);
        assert!(basis_bps > 800 && basis_bps < 900);
    }
}
