//! High-performance fixed-point statistics for trading
//!
//! Provides statistical functions using integer arithmetic with explicit scale/decimals
//! for maximum performance in trading systems.
//!
//! # Design
//!
//! - Uses i64/i128 internally for speed
//! - Scale (decimals) is tracked explicitly
//! - Rational numbers (num/denom) for fractions like EMA alpha
//! - No floating-point operations in hot paths

mod ema;
mod rolling;

pub use ema::Ema;
pub use rolling::RollingStats;

/// Scale factor for 8 decimal places (crypto standard)
pub const SCALE_8: i64 = 100_000_000;

/// Scale factor for 2 decimal places (fiat standard)
pub const SCALE_2: i64 = 100;

/// Integer square root using Newton-Raphson
#[inline]
pub fn isqrt(n: i128) -> i128 {
    if n < 0 {
        return 0;
    }
    if n < 2 {
        return n;
    }

    let mut x = n;
    let mut y = (x + 1) / 2;

    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Simple return calculation: (new - old) * scale / old
#[inline]
pub fn simple_return(old: i64, new: i64, scale: i64) -> Option<i64> {
    if old == 0 {
        return None;
    }
    Some(((new as i128 - old as i128) * scale as i128 / old as i128) as i64)
}

/// Calculate min of a slice
#[inline]
pub fn min(values: &[i64]) -> Option<i64> {
    values.iter().copied().min()
}

/// Calculate max of a slice
#[inline]
pub fn max(values: &[i64]) -> Option<i64> {
    values.iter().copied().max()
}

/// Calculate range (max - min)
#[inline]
pub fn range(values: &[i64]) -> Option<i64> {
    let min_val = min(values)?;
    let max_val = max(values)?;
    Some(max_val - min_val)
}

/// Calculate sum
#[inline]
pub fn sum(values: &[i64]) -> i128 {
    values.iter().map(|&x| x as i128).sum()
}

/// Calculate mean
#[inline]
pub fn mean(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    Some((sum(values) / values.len() as i128) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isqrt() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(100), 10);
        assert_eq!(isqrt(10000), 100);
    }

    #[test]
    fn test_simple_return() {
        // 10% return: (110 - 100) / 100 = 0.1
        let scale = 100_000_000i64;
        let ret = simple_return(100 * scale, 110 * scale, scale).unwrap();
        assert_eq!(ret, 10_000_000); // 0.1 scaled
    }

    #[test]
    fn test_mean() {
        let values = vec![100, 200, 300, 400, 500];
        assert_eq!(mean(&values), Some(300));
    }

    #[test]
    fn test_min_max_range() {
        let values = vec![5, 2, 8, 1, 9];
        assert_eq!(min(&values), Some(1));
        assert_eq!(max(&values), Some(9));
        assert_eq!(range(&values), Some(8));
    }
}
