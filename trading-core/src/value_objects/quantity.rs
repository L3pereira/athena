//! Fixed-point quantity representation for high-performance trading
//!
//! Uses i64 with 8 implied decimal places (scale = 100_000_000).
//! This provides ~92 quadrillion max value with 8 decimal precision.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

/// Scale factor: 8 decimal places (same as Price for consistency)
pub const QUANTITY_DECIMALS: u8 = 8;
pub const QUANTITY_SCALE: i64 = 100_000_000;

/// Fixed-point quantity with 8 decimal places
///
/// Internally stored as i64 where the value represents:
/// actual_quantity = raw_value / 100_000_000
///
/// Example: 123.45 is stored as 12_345_000_000
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Quantity(i64);

impl Quantity {
    pub const ZERO: Quantity = Quantity(0);
    pub const DECIMALS: u8 = QUANTITY_DECIMALS;
    pub const SCALE: i64 = QUANTITY_SCALE;

    /// Create from raw scaled value
    #[inline(always)]
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    /// Create from integer (whole number)
    #[inline(always)]
    pub const fn from_int(value: i64) -> Self {
        Self(value * QUANTITY_SCALE)
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

    /// Check if negative
    #[inline(always)]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
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

        let raw = int_part * QUANTITY_SCALE + frac_scaled;
        Ok(Self(if negative { -raw } else { raw }))
    }

    /// Round to lot size
    #[inline]
    pub fn round_to_lot(self, lot_size: Quantity) -> Quantity {
        if lot_size.is_zero() {
            return self;
        }
        let lots = self.0 / lot_size.0;
        Quantity(lots * lot_size.0)
    }

    /// Convert to f64 (for compatibility with external systems)
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / QUANTITY_SCALE as f64
    }

    /// Create from f64 (for compatibility)
    #[inline]
    pub fn from_f64(value: f64) -> Self {
        Self((value * QUANTITY_SCALE as f64) as i64)
    }

    /// Saturating subtraction (floors at zero)
    #[inline]
    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0).max(0))
    }

    /// Absolute value
    #[inline]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// Minimum of two quantities
    #[inline]
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// Maximum of two quantities
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }
}

impl Default for Quantity {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let int_part = self.0 / QUANTITY_SCALE;
        let frac_part = (self.0 % QUANTITY_SCALE).abs();
        write!(f, "{}.{:08}", int_part, frac_part)
    }
}

impl Add for Quantity {
    type Output = Quantity;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Quantity(self.0 + rhs.0)
    }
}

impl Sub for Quantity {
    type Output = Quantity;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Quantity(self.0 - rhs.0)
    }
}

impl Mul<i64> for Quantity {
    type Output = Quantity;
    #[inline(always)]
    fn mul(self, rhs: i64) -> Self::Output {
        Quantity(self.0 * rhs)
    }
}

impl Div<i64> for Quantity {
    type Output = Quantity;
    #[inline(always)]
    fn div(self, rhs: i64) -> Self::Output {
        Quantity(self.0 / rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_raw() {
        let q = Quantity::from_raw(12345_00000000);
        assert_eq!(q.raw(), 12345_00000000);
    }

    #[test]
    fn test_from_int() {
        let q = Quantity::from_int(100);
        assert_eq!(q.raw(), 100_00000000);
    }

    #[test]
    fn test_parse() {
        let q = Quantity::parse("123.45").unwrap();
        assert_eq!(q.raw(), 123_45000000);

        let q = Quantity::parse("0.00000001").unwrap();
        assert_eq!(q.raw(), 1);

        let q = Quantity::parse("1").unwrap();
        assert_eq!(q.raw(), 1_00000000);
    }

    #[test]
    fn test_display() {
        let q = Quantity::from_raw(123_45000000);
        assert_eq!(format!("{}", q), "123.45000000");
    }

    #[test]
    fn test_arithmetic() {
        let a = Quantity::from_int(100);
        let b = Quantity::from_int(50);

        assert_eq!((a + b).raw(), 150_00000000);
        assert_eq!((a - b).raw(), 50_00000000);
        assert_eq!((a * 2).raw(), 200_00000000);
        assert_eq!((a / 2).raw(), 50_00000000);
    }

    #[test]
    fn test_round_to_lot() {
        let q = Quantity::parse("123.456").unwrap();
        let lot = Quantity::parse("0.01").unwrap();
        let rounded = q.round_to_lot(lot);
        assert_eq!(rounded.raw(), 123_45000000);
    }

    #[test]
    fn test_saturating_sub() {
        let a = Quantity::from_int(10);
        let b = Quantity::from_int(20);
        assert_eq!(a.saturating_sub(b), Quantity::ZERO);
    }

    #[test]
    fn test_to_f64() {
        let q = Quantity::parse("123.45").unwrap();
        assert!((q.to_f64() - 123.45).abs() < 0.0000001);
    }
}
