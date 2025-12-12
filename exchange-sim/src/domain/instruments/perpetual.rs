use serde::{Deserialize, Serialize};

use super::InstrumentSpec;
use crate::domain::{PRICE_SCALE, Price, Quantity, Rate, Value};

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
    /// Contract multiplier (notional per contract) - scaled by PRICE_SCALE
    pub multiplier: i64,
    /// Initial margin requirement (in bps, e.g., 100 = 1%)
    pub initial_margin_bps: i64,
    /// Maintenance margin requirement (in bps, e.g., 50 = 0.5%)
    pub maintenance_margin_bps: i64,
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
            tick_size: Price::from_f64(0.01),
            lot_size: Quantity::from_f64(0.001),
            multiplier: PRICE_SCALE,    // 1.0 as fixed-point
            initial_margin_bps: 100,    // 1% = 100x max leverage
            maintenance_margin_bps: 50, // 0.5%
            funding_interval_hours: 8,
            is_inverse: false,
        }
    }

    /// Create a new inverse perpetual (settled in base currency)
    pub fn inverse(underlying: impl Into<String>, symbol: impl Into<String>) -> Self {
        Self {
            underlying: underlying.into(),
            symbol: symbol.into(),
            tick_size: Price::from_f64(0.5),
            lot_size: Quantity::from_f64(1.0), // Contract quantity
            multiplier: PRICE_SCALE,           // 1 USD per contract
            initial_margin_bps: 100,
            maintenance_margin_bps: 50,
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

    pub fn with_multiplier(mut self, mult: f64) -> Self {
        self.multiplier = (mult * PRICE_SCALE as f64) as i64;
        self
    }

    pub fn with_initial_margin_bps(mut self, margin_bps: i64) -> Self {
        self.initial_margin_bps = margin_bps;
        self
    }

    pub fn with_maintenance_margin_bps(mut self, margin_bps: i64) -> Self {
        self.maintenance_margin_bps = margin_bps;
        self
    }

    pub fn with_funding_interval(mut self, hours: u32) -> Self {
        self.funding_interval_hours = hours;
        self
    }

    /// Calculate position value for linear perp
    /// Value = price * quantity * multiplier
    pub fn position_value_linear(&self, price: Price, quantity: Quantity) -> Value {
        let notional = price.mul_qty(quantity);
        // Apply multiplier
        Value::from_raw(notional.raw() * self.multiplier as i128 / PRICE_SCALE as i128)
    }

    /// Calculate position value for inverse perp
    /// Value = quantity * multiplier / price (in base currency)
    pub fn position_value_inverse(&self, price: Price, quantity: Quantity) -> Value {
        if price.raw() == 0 {
            return Value::ZERO;
        }
        // quantity * multiplier / price
        let num = quantity.raw() as i128 * self.multiplier as i128;
        Value::from_raw(num / price.raw() as i128)
    }

    /// Calculate position value based on contract type
    pub fn position_value(&self, price: Price, quantity: Quantity) -> Value {
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
    ) -> Value {
        let direction: i128 = if is_long { 1 } else { -1 };

        if self.is_inverse {
            // Inverse PnL = qty * mult * (1/entry - 1/exit) for long
            // Compute: qty * mult * SCALE^2 / entry - qty * mult * SCALE^2 / exit
            // Then divide by SCALE to get final value
            if entry_price.raw() == 0 || exit_price.raw() == 0 {
                return Value::ZERO;
            }
            let qty_mult = quantity.raw() as i128 * self.multiplier as i128;
            let entry_inv = (PRICE_SCALE as i128 * PRICE_SCALE as i128) / entry_price.raw() as i128;
            let exit_inv = (PRICE_SCALE as i128 * PRICE_SCALE as i128) / exit_price.raw() as i128;
            let pnl = direction * qty_mult * (entry_inv - exit_inv) / PRICE_SCALE as i128;
            Value::from_raw(pnl / PRICE_SCALE as i128)
        } else {
            // Linear PnL = qty * mult * (exit - entry) for long
            let price_diff = exit_price.raw() as i128 - entry_price.raw() as i128;
            let pnl = direction * quantity.raw() as i128 * self.multiplier as i128 * price_diff
                / (PRICE_SCALE as i128 * PRICE_SCALE as i128);
            Value::from_raw(pnl)
        }
    }

    /// Maximum leverage based on initial margin
    pub fn max_leverage(&self) -> i64 {
        if self.initial_margin_bps == 0 {
            return 0;
        }
        10000 / self.initial_margin_bps // 100 bps = 1% = 100x leverage
    }

    /// Common perpetuals
    pub fn btc_perp() -> Self {
        Self::linear("BTC", "BTC-PERP")
            .with_tick_size(Price::from_f64(0.1))
            .with_lot_size(Quantity::from_f64(0.001))
    }

    pub fn eth_perp() -> Self {
        Self::linear("ETH", "ETH-PERP")
            .with_tick_size(Price::from_f64(0.01))
            .with_lot_size(Quantity::from_f64(0.01))
    }

    pub fn btcusd_inverse() -> Self {
        Self::inverse("BTC", "BTCUSD")
            .with_multiplier(100.0) // $100 per contract
            .with_tick_size(Price::from_f64(0.5))
            .with_lot_size(Quantity::from_f64(1.0))
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

    fn margin_requirement(&self) -> Rate {
        Rate::from_bps(self.initial_margin_bps)
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
        assert_eq!(perp.max_leverage(), 100);
    }

    #[test]
    fn test_inverse_perp() {
        let perp = PerpetualContract::btcusd_inverse();
        assert!(perp.is_inverse);
        // multiplier is 100 * PRICE_SCALE
        assert_eq!(perp.multiplier, 100 * PRICE_SCALE);
    }

    #[test]
    fn test_linear_pnl() {
        let perp = PerpetualContract::linear("BTC", "BTC-PERP");

        // Long 1 BTC, entry 50000, exit 51000 = +1000 profit
        let pnl = perp.calculate_pnl(
            Price::from_int(50000),
            Price::from_int(51000),
            Quantity::from_int(1),
            true,
        );
        assert_eq!(pnl, Value::from_int(1000));

        // Short 1 BTC, entry 50000, exit 51000 = -1000 loss
        let pnl = perp.calculate_pnl(
            Price::from_int(50000),
            Price::from_int(51000),
            Quantity::from_int(1),
            false,
        );
        assert_eq!(pnl, Value::from_int(-1000));
    }

    #[test]
    fn test_inverse_pnl() {
        let perp = PerpetualContract::inverse("BTC", "BTCUSD").with_multiplier(100.0);

        // Long 100 contracts ($100 each), entry 50000, exit 51000
        // PnL should be positive for long when price goes up
        let pnl = perp.calculate_pnl(
            Price::from_int(50000),
            Price::from_int(51000),
            Quantity::from_int(100),
            true,
        );
        assert!(pnl.raw() > 0);
    }

    #[test]
    fn test_position_value() {
        let linear = PerpetualContract::btc_perp();
        let value = linear.position_value(Price::from_int(50000), Quantity::from_int(1));
        assert_eq!(value, Value::from_int(50000));

        let inverse = PerpetualContract::btcusd_inverse();
        // 100 contracts at $50000 = 100 * 100 / 50000 = 0.2 BTC
        let value = inverse.position_value(Price::from_int(50000), Quantity::from_int(100));
        // Expected: 0.2 BTC = 0.2 * PRICE_SCALE
        assert_eq!(value.to_f64(), 0.2);
    }
}
