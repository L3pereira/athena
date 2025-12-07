use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Quantity(Decimal);

impl Quantity {
    pub const ZERO: Quantity = Quantity(Decimal::ZERO);

    pub fn new(value: Decimal) -> Result<Self, &'static str> {
        if value < Decimal::ZERO {
            return Err("Quantity cannot be negative");
        }
        Ok(Quantity(value))
    }

    pub fn parse(s: &str) -> Result<Self, rust_decimal::Error> {
        let decimal = s.parse::<Decimal>()?;
        Ok(Quantity(decimal))
    }

    pub fn inner(&self) -> Decimal {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn round_to_lot(&self, lot_size: Quantity) -> Quantity {
        if lot_size.is_zero() {
            return *self;
        }
        let lots = (self.0 / lot_size.0).floor();
        Quantity(lots * lot_size.0)
    }

    pub fn saturating_sub(self, rhs: Self) -> Self {
        if self.0 >= rhs.0 {
            Quantity(self.0 - rhs.0)
        } else {
            Quantity::ZERO
        }
    }
}

impl From<Decimal> for Quantity {
    fn from(value: Decimal) -> Self {
        Quantity(value)
    }
}

impl From<Quantity> for Decimal {
    fn from(qty: Quantity) -> Decimal {
        qty.0
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Add for Quantity {
    type Output = Quantity;
    fn add(self, rhs: Self) -> Self::Output {
        Quantity(self.0 + rhs.0)
    }
}

impl Sub for Quantity {
    type Output = Quantity;
    fn sub(self, rhs: Self) -> Self::Output {
        Quantity(self.0 - rhs.0)
    }
}

impl Mul<Decimal> for Quantity {
    type Output = Quantity;
    fn mul(self, rhs: Decimal) -> Self::Output {
        Quantity(self.0 * rhs)
    }
}

impl Div<Decimal> for Quantity {
    type Output = Quantity;
    fn div(self, rhs: Decimal) -> Self::Output {
        Quantity(self.0 / rhs)
    }
}

impl Default for Quantity {
    fn default() -> Self {
        Quantity::ZERO
    }
}
