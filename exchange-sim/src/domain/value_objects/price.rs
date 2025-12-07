use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Price(Decimal);

impl Price {
    pub const ZERO: Price = Price(Decimal::ZERO);

    pub fn new(value: Decimal) -> Result<Self, &'static str> {
        if value < Decimal::ZERO {
            return Err("Price cannot be negative");
        }
        Ok(Price(value))
    }

    pub fn parse(s: &str) -> Result<Self, rust_decimal::Error> {
        let decimal = s.parse::<Decimal>()?;
        Ok(Price(decimal))
    }

    pub fn inner(&self) -> Decimal {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn round_to_tick(&self, tick_size: Price) -> Price {
        if tick_size.is_zero() {
            return *self;
        }
        let ticks = (self.0 / tick_size.0).floor();
        Price(ticks * tick_size.0)
    }
}

impl From<Decimal> for Price {
    fn from(value: Decimal) -> Self {
        Price(value)
    }
}

impl From<Price> for Decimal {
    fn from(price: Price) -> Decimal {
        price.0
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Add for Price {
    type Output = Price;
    fn add(self, rhs: Self) -> Self::Output {
        Price(self.0 + rhs.0)
    }
}

impl Sub for Price {
    type Output = Price;
    fn sub(self, rhs: Self) -> Self::Output {
        Price(self.0 - rhs.0)
    }
}

impl Mul<Decimal> for Price {
    type Output = Price;
    fn mul(self, rhs: Decimal) -> Self::Output {
        Price(self.0 * rhs)
    }
}

impl Div<Decimal> for Price {
    type Output = Price;
    fn div(self, rhs: Decimal) -> Self::Output {
        Price(self.0 / rhs)
    }
}

impl Default for Price {
    fn default() -> Self {
        Price::ZERO
    }
}
