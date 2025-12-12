//! Signal-specific value objects
//!
//! Type-safe wrappers for domain values that prevent mixing up
//! different concepts and make code self-documenting.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Neg, Sub};
use trading_core::{Price, Quantity};

/// Basis points (1 bp = 0.01%)
///
/// Used for spread, edge, and price deviations.
/// Stored as scaled integer (100 = 1 bp for precision)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
pub struct BasisPoints(i64);

/// Scale for basis points: 100 internal units = 1 bp
const BP_SCALE: i64 = 100;

impl BasisPoints {
    pub const ZERO: BasisPoints = BasisPoints(0);

    /// Create from basis points value (e.g., 50 = 50 bps)
    pub fn new(bps: f64) -> Self {
        Self((bps * BP_SCALE as f64) as i64)
    }

    /// Create from raw scaled value
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    /// Create from a decimal ratio (e.g., 0.005 = 50 bps)
    pub fn from_ratio(ratio: f64) -> Self {
        Self((ratio * 10000.0 * BP_SCALE as f64) as i64)
    }

    /// Create from two prices: (price - reference) / reference * 10000
    pub fn from_price_diff(price: Price, reference: Price) -> Option<Self> {
        if reference.is_zero() {
            return None;
        }
        // (price - reference) / reference * 10000 * BP_SCALE
        let diff = price.raw() as i128 - reference.raw() as i128;
        let scaled = (diff * 10000 * BP_SCALE as i128) / reference.raw() as i128;
        Some(Self(scaled as i64))
    }

    /// Get value in basis points (e.g., 50 for 50 bps)
    pub fn value(&self) -> f64 {
        self.0 as f64 / BP_SCALE as f64
    }

    /// Get raw scaled value
    pub const fn raw(&self) -> i64 {
        self.0
    }

    /// Convert to decimal ratio (50 bps -> 0.005)
    pub fn to_ratio(&self) -> f64 {
        self.0 as f64 / (10000.0 * BP_SCALE as f64)
    }

    /// Convert to percentage (50 bps -> 0.5%)
    pub fn to_percent(&self) -> f64 {
        self.0 as f64 / (100.0 * BP_SCALE as f64)
    }

    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }

    pub fn is_positive(&self) -> bool {
        self.0 > 0
    }

    pub fn is_negative(&self) -> bool {
        self.0 < 0
    }
}

impl fmt::Display for BasisPoints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}bp", self.value())
    }
}

impl Add for BasisPoints {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for BasisPoints {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Neg for BasisPoints {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

/// Bounded ratio in [-1, 1]
///
/// Used for imbalance, book pressure, and other normalized metrics.
/// Positive typically means buying pressure, negative means selling pressure.
/// Stored as scaled integer (RATIO_SCALE = 1.0)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
pub struct Ratio(i64);

/// Scale for ratio: 100_000_000 = 1.0
const RATIO_SCALE: i64 = 100_000_000;

impl Ratio {
    pub const ZERO: Ratio = Ratio(0);
    pub const ONE: Ratio = Ratio(RATIO_SCALE);
    pub const NEG_ONE: Ratio = Ratio(-RATIO_SCALE);

    /// Create zero ratio
    pub fn zero() -> Self {
        Self::ZERO
    }

    /// Create a ratio, clamping to [-1, 1]
    pub fn new(value: f64) -> Self {
        let clamped = value.clamp(-1.0, 1.0);
        Self((clamped * RATIO_SCALE as f64) as i64)
    }

    /// Create from raw scaled value (already scaled by RATIO_SCALE)
    pub fn from_raw(raw: i64) -> Self {
        Self(raw.clamp(-RATIO_SCALE, RATIO_SCALE))
    }

    /// Create from numerator/denominator (using Quantity)
    pub fn from_fraction(numerator: Quantity, denominator: Quantity) -> Self {
        if denominator.is_zero() {
            return Self::ZERO;
        }
        let ratio = (numerator.raw() as i128 * RATIO_SCALE as i128) / denominator.raw() as i128;
        Self::from_raw(ratio as i64)
    }

    /// Create imbalance: (a - b) / (a + b)
    pub fn imbalance(a: Quantity, b: Quantity) -> Self {
        let total = a.raw() as i128 + b.raw() as i128;
        if total == 0 {
            return Self::ZERO;
        }
        let diff = a.raw() as i128 - b.raw() as i128;
        let ratio = (diff * RATIO_SCALE as i128) / total;
        Self::from_raw(ratio as i64)
    }

    /// Get value as f64 in [-1, 1]
    pub fn value(&self) -> f64 {
        self.0 as f64 / RATIO_SCALE as f64
    }

    /// Get raw scaled value
    pub const fn raw(&self) -> i64 {
        self.0
    }

    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }

    pub fn is_positive(&self) -> bool {
        self.0 > 0
    }

    pub fn is_negative(&self) -> bool {
        self.0 < 0
    }

    pub fn is_neutral(&self) -> bool {
        self.0.abs() < RATIO_SCALE / 100 // |value| < 0.01
    }
}

impl fmt::Display for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}", self.value())
    }
}

/// Confidence score in [0, 1]
///
/// Represents model confidence in a prediction.
/// Stored as scaled integer (CONFIDENCE_SCALE = 1.0)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Confidence(i64);

const CONFIDENCE_SCALE: i64 = 100_000_000;

impl Confidence {
    pub const ZERO: Confidence = Confidence(0);
    pub const FULL: Confidence = Confidence(CONFIDENCE_SCALE);

    /// Create confidence, clamping to [0, 1]
    pub fn new(value: f64) -> Self {
        let clamped = value.clamp(0.0, 1.0);
        Self((clamped * CONFIDENCE_SCALE as f64) as i64)
    }

    /// Create from raw scaled value
    pub const fn from_raw(raw: i64) -> Self {
        Self(if raw < 0 {
            0
        } else if raw > CONFIDENCE_SCALE {
            CONFIDENCE_SCALE
        } else {
            raw
        })
    }

    pub fn value(&self) -> f64 {
        self.0 as f64 / CONFIDENCE_SCALE as f64
    }

    pub const fn raw(&self) -> i64 {
        self.0
    }

    /// High confidence (> 0.8)
    pub fn is_high(&self) -> bool {
        self.0 > CONFIDENCE_SCALE * 8 / 10
    }

    /// Medium confidence (0.5 - 0.8)
    pub fn is_medium(&self) -> bool {
        self.0 >= CONFIDENCE_SCALE / 2 && self.0 <= CONFIDENCE_SCALE * 8 / 10
    }

    /// Low confidence (< 0.5)
    pub fn is_low(&self) -> bool {
        self.0 < CONFIDENCE_SCALE / 2
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1}%", self.value() * 100.0)
    }
}

/// Signal strength in [-1, 1]
///
/// Indicates how strong the trading signal is.
/// Positive = buy pressure, negative = sell pressure.
/// Magnitude indicates size recommendation.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Strength(i64);

const STRENGTH_SCALE: i64 = 100_000_000;

impl Strength {
    pub const ZERO: Strength = Strength(0);
    pub const MAX_BUY: Strength = Strength(STRENGTH_SCALE);
    pub const MAX_SELL: Strength = Strength(-STRENGTH_SCALE);

    /// Create strength, clamping to [-1, 1]
    pub fn new(value: f64) -> Self {
        let clamped = value.clamp(-1.0, 1.0);
        Self((clamped * STRENGTH_SCALE as f64) as i64)
    }

    /// Create from raw scaled value
    pub fn from_raw(raw: i64) -> Self {
        Self(raw.clamp(-STRENGTH_SCALE, STRENGTH_SCALE))
    }

    pub fn value(&self) -> f64 {
        self.0 as f64 / STRENGTH_SCALE as f64
    }

    pub fn raw(&self) -> i64 {
        self.0
    }

    pub fn abs(&self) -> f64 {
        (self.0.abs() as f64) / STRENGTH_SCALE as f64
    }

    pub fn is_buy(&self) -> bool {
        self.0 > 0
    }

    pub fn is_sell(&self) -> bool {
        self.0 < 0
    }

    pub fn is_neutral(&self) -> bool {
        self.0.abs() < STRENGTH_SCALE / 100 // |value| < 0.01
    }

    /// Strong signal (|strength| > 0.7)
    pub fn is_strong(&self) -> bool {
        self.0.abs() > STRENGTH_SCALE * 7 / 10
    }
}

impl Default for Strength {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Strength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let direction = if self.is_buy() {
            "BUY"
        } else if self.is_sell() {
            "SELL"
        } else {
            "NEUTRAL"
        };
        write!(f, "{} {:.0}%", direction, self.abs() * 100.0)
    }
}

impl Neg for Strength {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

/// Volatility (annualized standard deviation)
///
/// Non-negative value representing price volatility.
/// Stored as scaled integer (VOLATILITY_SCALE = 1.0 = 100%)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
pub struct Volatility(i64);

const VOLATILITY_SCALE: i64 = 100_000_000;

impl Volatility {
    pub const ZERO: Volatility = Volatility(0);

    /// Create volatility (must be non-negative)
    pub fn new(value: f64) -> Self {
        Self((value.max(0.0) * VOLATILITY_SCALE as f64) as i64)
    }

    /// Create from raw scaled value
    pub const fn from_raw(raw: i64) -> Self {
        Self(if raw < 0 { 0 } else { raw })
    }

    pub fn value(&self) -> f64 {
        self.0 as f64 / VOLATILITY_SCALE as f64
    }

    pub const fn raw(&self) -> i64 {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// High volatility (> 50% annualized)
    pub fn is_high(&self) -> bool {
        self.0 > VOLATILITY_SCALE / 2
    }

    /// Low volatility (< 10% annualized)
    pub fn is_low(&self) -> bool {
        self.0 < VOLATILITY_SCALE / 10
    }
}

impl fmt::Display for Volatility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}%", self.value() * 100.0)
    }
}

/// Z-score (standard deviations from mean)
///
/// Used for mean reversion signals.
/// Stored as scaled integer (ZSCORE_SCALE = 1 sigma)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
pub struct ZScore(i64);

const ZSCORE_SCALE: i64 = 100_000_000;

impl ZScore {
    pub const ZERO: ZScore = ZScore(0);

    pub fn new(value: f64) -> Self {
        Self((value * ZSCORE_SCALE as f64) as i64)
    }

    /// Create from raw scaled value
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    /// Calculate z-score from value, mean, and std (all scaled by PRICE_SCALE)
    pub fn calculate_scaled(value: i64, mean: i64, std: i64) -> Option<Self> {
        if std == 0 {
            return None;
        }
        // z = (value - mean) / std
        let diff = value as i128 - mean as i128;
        let z = (diff * ZSCORE_SCALE as i128) / std as i128;
        Some(Self(z as i64))
    }

    /// Calculate z-score from f64 values
    pub fn calculate(value: f64, mean: f64, std: f64) -> Option<Self> {
        if std < 1e-10 {
            return None;
        }
        Some(Self::new((value - mean) / std))
    }

    pub fn value(&self) -> f64 {
        self.0 as f64 / ZSCORE_SCALE as f64
    }

    pub const fn raw(&self) -> i64 {
        self.0
    }

    pub fn abs(&self) -> f64 {
        (self.0.abs() as f64) / ZSCORE_SCALE as f64
    }

    /// Extreme deviation (|z| > 2)
    pub fn is_extreme(&self) -> bool {
        self.0.abs() >= 2 * ZSCORE_SCALE
    }

    /// Significant deviation (|z| > 1)
    pub fn is_significant(&self) -> bool {
        self.0.abs() > ZSCORE_SCALE
    }
}

impl fmt::Display for ZScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}σ", self.value())
    }
}

impl Neg for ZScore {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basis_points() {
        let bps = BasisPoints::new(50.0);
        assert!((bps.value() - 50.0).abs() < 0.01);
        assert!((bps.to_percent() - 0.5).abs() < 0.001);
        assert!((bps.to_ratio() - 0.005).abs() < 0.0001);

        let from_ratio = BasisPoints::from_ratio(0.01); // 1%
        assert!((from_ratio.value() - 100.0).abs() < 0.1);

        let diff =
            BasisPoints::from_price_diff(Price::from_int(101), Price::from_int(100)).unwrap();
        assert!((diff.value() - 100.0).abs() < 0.1); // 1% = 100 bps
    }

    #[test]
    fn test_ratio() {
        let ratio = Ratio::new(0.5);
        assert!(ratio.is_positive());
        assert!(!ratio.is_negative());

        // Test clamping
        let clamped = Ratio::new(1.5);
        assert!((clamped.value() - 1.0).abs() < 0.001);

        // Test imbalance
        let imb = Ratio::imbalance(Quantity::from_int(10), Quantity::from_int(5));
        assert!((imb.value() - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_confidence() {
        let high = Confidence::new(0.9);
        assert!(high.is_high());
        assert!(!high.is_low());

        let low = Confidence::new(0.3);
        assert!(low.is_low());

        // Test clamping
        let clamped = Confidence::new(1.5);
        assert!((clamped.value() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_strength() {
        let buy = Strength::new(0.8);
        assert!(buy.is_buy());
        assert!(buy.is_strong());

        let sell = Strength::new(-0.5);
        assert!(sell.is_sell());
        assert!(!sell.is_strong());

        let neutral = Strength::new(0.005);
        assert!(neutral.is_neutral());
    }

    #[test]
    fn test_z_score() {
        let z = ZScore::calculate(110.0, 100.0, 5.0).unwrap();
        assert!((z.value() - 2.0).abs() < 0.01);
        assert!(z.is_extreme());
        assert!(z.is_significant());
    }

    #[test]
    fn test_volatility() {
        let vol = Volatility::new(0.25); // 25%
        assert!(!vol.is_high());
        assert!(!vol.is_low());

        let high_vol = Volatility::new(0.6);
        assert!(high_vol.is_high());
    }
}
