//! Fixed-point price representation for high-performance trading
//!
//! Uses i64 with 8 implied decimal places (scale = 100_000_000).
//! This provides ~92 quadrillion max value with 8 decimal precision.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

/// Scale factor: 8 decimal places
pub const PRICE_DECIMALS: u8 = 8;
pub const PRICE_SCALE: i64 = 100_000_000;

/// Fixed-point price with 8 decimal places
///
/// Internally stored as i64 where the value represents:
/// actual_price = raw_value / 100_000_000
///
/// Example: 123.45 is stored as 12_345_000_000
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Price(i64);

impl Price {
    pub const ZERO: Price = Price(0);
    pub const DECIMALS: u8 = PRICE_DECIMALS;
    pub const SCALE: i64 = PRICE_SCALE;

    /// Create from raw scaled value
    #[inline(always)]
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    /// Create from integer (whole number)
    #[inline(always)]
    pub const fn from_int(value: i64) -> Self {
        Self(value * PRICE_SCALE)
    }

    /// Get the raw scaled value
    #[inline(always)]
    pub const fn raw(self) -> i64 {
        self.0
    }

    /// Check if zero
    #[inline(always)]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Parse from string (e.g., "123.45678901")
    pub fn parse(s: &str) -> Result<Self, &'static str> {
        let s = s.trim();
        if s.is_empty() {
            return Err("Empty string");
        }

        let negative = s.starts_with('-');
        let s = if negative { &s[1..] } else { s };

        let mut parts = s.split('.');
        let int_part: i64 = parts
            .next()
            .ok_or("Missing integer part")?
            .parse()
            .map_err(|_| "Invalid integer part")?;

        let frac_scaled = if let Some(frac_str) = parts.next() {
            if frac_str.len() > 8 {
                // Truncate to 8 decimal places
                let truncated = &frac_str[..8];
                truncated.parse::<i64>().map_err(|_| "Invalid fraction")?
            } else {
                let frac: i64 = frac_str.parse().map_err(|_| "Invalid fraction")?;
                // Scale up to 8 decimal places
                frac * 10i64.pow(8 - frac_str.len() as u32)
            }
        } else {
            0
        };

        let raw = int_part * PRICE_SCALE + frac_scaled;
        Ok(Self(if negative { -raw } else { raw }))
    }

    /// Round to tick size
    #[inline]
    pub fn round_to_tick(self, tick_size: Price) -> Price {
        if tick_size.is_zero() {
            return self;
        }
        let ticks = self.0 / tick_size.0;
        Price(ticks * tick_size.0)
    }

    /// Multiply by quantity, returning Value (i128 for overflow safety)
    #[inline(always)]
    pub const fn mul_qty(self, qty: super::Quantity) -> Value {
        // price (8 dec) × qty (8 dec) = 16 dec, divide by SCALE to get 8 dec
        Value::from_raw((self.0 as i128 * qty.raw() as i128) / PRICE_SCALE as i128)
    }

    /// Convert to f64 (for compatibility with external systems)
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / PRICE_SCALE as f64
    }

    /// Create from f64 (for compatibility)
    #[inline]
    pub fn from_f64(value: f64) -> Self {
        Self((value * PRICE_SCALE as f64) as i64)
    }

    /// Saturating subtraction
    #[inline]
    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    /// Absolute value
    #[inline]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl Default for Price {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let int_part = self.0 / PRICE_SCALE;
        let frac_part = (self.0 % PRICE_SCALE).abs();
        write!(f, "{}.{:08}", int_part, frac_part)
    }
}

impl Add for Price {
    type Output = Price;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Price(self.0 + rhs.0)
    }
}

impl Sub for Price {
    type Output = Price;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Price(self.0 - rhs.0)
    }
}

impl Mul<i64> for Price {
    type Output = Price;
    #[inline(always)]
    fn mul(self, rhs: i64) -> Self::Output {
        Price(self.0 * rhs)
    }
}

impl Div<i64> for Price {
    type Output = Price;
    #[inline(always)]
    fn div(self, rhs: i64) -> Self::Output {
        Price(self.0 / rhs)
    }
}

/// Value type for price × quantity results
///
/// Uses i128 to handle the larger range from multiplication.
/// Stored with 8 decimal places like Price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Value(i128);

impl Value {
    pub const ZERO: Value = Value(0);
    pub const DECIMALS: u8 = PRICE_DECIMALS;
    pub const SCALE: i64 = PRICE_SCALE;

    #[inline(always)]
    pub const fn from_raw(raw: i128) -> Self {
        Self(raw)
    }

    #[inline(always)]
    pub const fn raw(self) -> i128 {
        self.0
    }

    #[inline(always)]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Create from integer value (e.g., 100 -> 100.00000000)
    #[inline]
    pub const fn from_int(value: i64) -> Self {
        Self(value as i128 * PRICE_SCALE as i128)
    }

    /// Create from f64 value (e.g., 1.5 -> 1.50000000)
    #[inline]
    pub fn from_f64(value: f64) -> Self {
        Self((value * PRICE_SCALE as f64) as i128)
    }

    /// Convert to f64
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / PRICE_SCALE as f64
    }
}

impl Add for Value {
    type Output = Value;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Value(self.0 + rhs.0)
    }
}

impl Sub for Value {
    type Output = Value;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Value(self.0 - rhs.0)
    }
}

impl Default for Value {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let int_part = self.0 / PRICE_SCALE as i128;
        let frac_part = (self.0 % PRICE_SCALE as i128).abs();
        write!(f, "{}.{:08}", int_part, frac_part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_raw() {
        let p = Price::from_raw(12345_00000000);
        assert_eq!(p.raw(), 12345_00000000);
    }

    #[test]
    fn test_from_int() {
        let p = Price::from_int(100);
        assert_eq!(p.raw(), 100_00000000);
    }

    #[test]
    fn test_parse() {
        let p = Price::parse("123.45").unwrap();
        assert_eq!(p.raw(), 123_45000000);

        let p = Price::parse("0.00000001").unwrap();
        assert_eq!(p.raw(), 1);

        let p = Price::parse("1").unwrap();
        assert_eq!(p.raw(), 1_00000000);
    }

    #[test]
    fn test_display() {
        let p = Price::from_raw(123_45000000);
        assert_eq!(format!("{}", p), "123.45000000");
    }

    #[test]
    fn test_arithmetic() {
        let a = Price::from_int(100);
        let b = Price::from_int(50);

        assert_eq!((a + b).raw(), 150_00000000);
        assert_eq!((a - b).raw(), 50_00000000);
        assert_eq!((a * 2).raw(), 200_00000000);
        assert_eq!((a / 2).raw(), 50_00000000);
    }

    #[test]
    fn test_round_to_tick() {
        let p = Price::parse("123.456").unwrap();
        let tick = Price::parse("0.01").unwrap();
        let rounded = p.round_to_tick(tick);
        assert_eq!(rounded.raw(), 123_45000000);
    }

    #[test]
    fn test_to_f64() {
        let p = Price::parse("123.45").unwrap();
        assert!((p.to_f64() - 123.45).abs() < 0.0000001);
    }
}
