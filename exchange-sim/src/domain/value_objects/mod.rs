// Re-export all value objects from trading-core
pub use trading_core::value_objects::{
    OrderId, OrderType, PRICE_SCALE, Price, QUANTITY_SCALE, Quantity, Side, Symbol, TimeInForce,
    Timestamp, TradeId, Value,
};

/// Basis points scale (10000 = 100%)
pub const BPS_SCALE: i64 = 10_000;

/// Rate type for fee rates, margin rates, etc. stored in basis points
/// e.g., 10 = 0.1%, 100 = 1%, 1000 = 10%
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Rate(i64);

impl Rate {
    pub const ZERO: Rate = Rate(0);
    pub const ONE: Rate = Rate(BPS_SCALE); // 100%

    pub const fn from_bps(bps: i64) -> Self {
        Self(bps)
    }

    /// Create from percentage (e.g., 10.0 = 10%)
    pub fn from_percent(pct: f64) -> Self {
        Self((pct * 100.0) as i64)
    }

    pub const fn bps(self) -> i64 {
        self.0
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / BPS_SCALE as f64
    }

    /// Apply rate to a Value: value * rate
    pub fn apply_to_value(self, value: Value) -> Value {
        Value::from_raw(value.raw() * self.0 as i128 / BPS_SCALE as i128)
    }

    /// Multiply two rates (e.g., for discount)
    pub fn mul_rate(self, other: Rate) -> Rate {
        Rate((self.0 as i128 * other.0 as i128 / BPS_SCALE as i128) as i64)
    }
}

impl std::ops::Mul<Rate> for Value {
    type Output = Value;
    fn mul(self, rate: Rate) -> Value {
        rate.apply_to_value(self)
    }
}
