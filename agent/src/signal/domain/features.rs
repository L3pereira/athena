//! Feature-related value objects

use super::SignalDirection;
use super::value_objects::{BasisPoints, Ratio, Volatility, ZScore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use trading_core::Price;

/// Signal urgency level - how quickly the edge decays
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Urgency(f64);

impl Urgency {
    /// Create urgency value (clamped to [0, 1])
    pub fn new(value: f64) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// High urgency - edge decays quickly (e.g., latency arb)
    pub fn high() -> Self {
        Self(1.0)
    }

    /// Medium urgency - moderate decay
    pub fn medium() -> Self {
        Self(0.5)
    }

    /// Low urgency - edge is persistent (e.g., cointegration)
    pub fn low() -> Self {
        Self(0.2)
    }

    /// Get the underlying value
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Returns true if urgency is above the given threshold
    pub fn above(&self, threshold: f64) -> bool {
        self.0 > threshold
    }
}

impl Default for Urgency {
    fn default() -> Self {
        Self::medium()
    }
}

/// A leg in a multi-leg strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leg {
    /// Trading symbol
    pub symbol: String,
    /// Direction for this leg
    pub direction: SignalDirection,
    /// Hedge ratio or weight (scaled by 100_000_000 for 8 decimal precision)
    pub ratio: i64,
    /// Target venue/exchange
    pub venue: String,
}

impl Leg {
    /// Create a new leg
    pub fn new(
        symbol: impl Into<String>,
        direction: SignalDirection,
        ratio: f64,
        venue: impl Into<String>,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            direction,
            ratio: (ratio * 100_000_000.0) as i64,
            venue: venue.into(),
        }
    }

    /// Create a buy leg
    pub fn buy(symbol: impl Into<String>, ratio: f64, venue: impl Into<String>) -> Self {
        Self::new(symbol, SignalDirection::Buy, ratio, venue)
    }

    /// Create a sell leg
    pub fn sell(symbol: impl Into<String>, ratio: f64, venue: impl Into<String>) -> Self {
        Self::new(symbol, SignalDirection::Sell, ratio, venue)
    }

    /// Get ratio as f64
    pub fn ratio_f64(&self) -> f64 {
        self.ratio as f64 / 100_000_000.0
    }
}

/// Container for extracted features with typed values
///
/// Separates features by type for precision and type safety:
/// - `prices`: Price-related features (mid, microprice, vwap) using `Price`
/// - `ratios`: Bounded ratios (imbalance, pressure) using `Ratio`
/// - `basis_points`: Spreads and deviations using `BasisPoints`
/// - `statistics`: Pure statistical values (volatility, z-score, etc.)
/// - `raw`: Legacy f64 values for backward compatibility
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Features {
    /// Price features (preserve decimal precision)
    #[serde(default)]
    prices: HashMap<String, Price>,
    /// Ratio features [-1, 1]
    #[serde(default)]
    ratios: HashMap<String, Ratio>,
    /// Basis point features
    #[serde(default)]
    basis_points: HashMap<String, BasisPoints>,
    /// Volatility features
    #[serde(default)]
    volatilities: HashMap<String, Volatility>,
    /// Z-score features
    #[serde(default)]
    z_scores: HashMap<String, ZScore>,
    /// Raw f64 values (legacy compatibility + pure statistics)
    #[serde(default)]
    raw: HashMap<String, f64>,
}

impl Features {
    /// Create empty features
    pub fn new() -> Self {
        Self::default()
    }

    // === Typed setters ===

    /// Set a price feature
    pub fn set_price(&mut self, name: impl Into<String>, value: Price) {
        self.prices.insert(name.into(), value);
    }

    /// Set a ratio feature
    pub fn set_ratio(&mut self, name: impl Into<String>, value: Ratio) {
        self.ratios.insert(name.into(), value);
    }

    /// Set a basis points feature
    pub fn set_bps(&mut self, name: impl Into<String>, value: BasisPoints) {
        self.basis_points.insert(name.into(), value);
    }

    /// Set a volatility feature
    pub fn set_volatility(&mut self, name: impl Into<String>, value: Volatility) {
        self.volatilities.insert(name.into(), value);
    }

    /// Set a z-score feature
    pub fn set_zscore(&mut self, name: impl Into<String>, value: ZScore) {
        self.z_scores.insert(name.into(), value);
    }

    // === Typed getters ===

    /// Get a price feature
    pub fn get_price(&self, name: &str) -> Option<Price> {
        self.prices.get(name).copied()
    }

    /// Get a ratio feature
    pub fn get_ratio(&self, name: &str) -> Option<Ratio> {
        self.ratios.get(name).copied()
    }

    /// Get a basis points feature
    pub fn get_bps(&self, name: &str) -> Option<BasisPoints> {
        self.basis_points.get(name).copied()
    }

    /// Get a volatility feature
    pub fn get_volatility(&self, name: &str) -> Option<Volatility> {
        self.volatilities.get(name).copied()
    }

    /// Get a z-score feature
    pub fn get_zscore(&self, name: &str) -> Option<ZScore> {
        self.z_scores.get(name).copied()
    }

    // === Legacy f64 API (backward compatibility) ===

    /// Add a raw f64 feature (legacy)
    pub fn insert(&mut self, name: impl Into<String>, value: f64) {
        self.raw.insert(name.into(), value);
    }

    /// Add a raw f64 feature (alias for insert)
    pub fn set(&mut self, name: impl Into<String>, value: f64) {
        self.raw.insert(name.into(), value);
    }

    /// Get a raw f64 feature value
    pub fn get(&self, name: &str) -> Option<f64> {
        self.raw.get(name).copied()
    }

    /// Check if any feature with this name exists
    pub fn contains(&self, name: &str) -> bool {
        self.prices.contains_key(name)
            || self.ratios.contains_key(name)
            || self.basis_points.contains_key(name)
            || self.volatilities.contains_key(name)
            || self.z_scores.contains_key(name)
            || self.raw.contains_key(name)
    }

    /// Merge another Features into this one
    pub fn merge(&mut self, other: Features) {
        self.prices.extend(other.prices);
        self.ratios.extend(other.ratios);
        self.basis_points.extend(other.basis_points);
        self.volatilities.extend(other.volatilities);
        self.z_scores.extend(other.z_scores);
        self.raw.extend(other.raw);
    }

    /// Merge from a raw HashMap (legacy)
    pub fn merge_map(&mut self, other: HashMap<String, f64>) {
        self.raw.extend(other);
    }

    /// Get raw values map (legacy)
    pub fn as_map(&self) -> &HashMap<String, f64> {
        &self.raw
    }

    /// Convert raw values to HashMap (legacy)
    pub fn into_map(self) -> HashMap<String, f64> {
        self.raw
    }

    /// Total number of features across all types
    pub fn len(&self) -> usize {
        self.prices.len()
            + self.ratios.len()
            + self.basis_points.len()
            + self.volatilities.len()
            + self.z_scores.len()
            + self.raw.len()
    }

    /// Check if all feature maps are empty
    pub fn is_empty(&self) -> bool {
        self.prices.is_empty()
            && self.ratios.is_empty()
            && self.basis_points.is_empty()
            && self.volatilities.is_empty()
            && self.z_scores.is_empty()
            && self.raw.is_empty()
    }

    /// Convert all typed features to f64 map (for ML pipelines)
    pub fn to_f64_map(&self) -> HashMap<String, f64> {
        let mut result = self.raw.clone();

        for (k, v) in &self.prices {
            result.insert(k.clone(), v.to_f64());
        }
        for (k, v) in &self.ratios {
            result.insert(k.clone(), v.value());
        }
        for (k, v) in &self.basis_points {
            result.insert(k.clone(), v.value());
        }
        for (k, v) in &self.volatilities {
            result.insert(k.clone(), v.value());
        }
        for (k, v) in &self.z_scores {
            result.insert(k.clone(), v.value());
        }

        result
    }

    /// Create from raw f64 map (legacy)
    pub fn from_map(values: HashMap<String, f64>) -> Self {
        Self {
            raw: values,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urgency() {
        let high = Urgency::high();
        let low = Urgency::low();

        assert_eq!(high.value(), 1.0);
        assert_eq!(low.value(), 0.2);
        assert!(high.above(0.5));
        assert!(!low.above(0.5));

        // Clamping
        let clamped = Urgency::new(1.5);
        assert_eq!(clamped.value(), 1.0);
    }

    #[test]
    fn test_leg() {
        let leg = Leg::buy("BTCUSDT", 1.0, "binance");
        assert_eq!(leg.symbol, "BTCUSDT");
        assert_eq!(leg.direction, SignalDirection::Buy);
        assert!((leg.ratio_f64() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_features() {
        let mut features = Features::new();
        features.insert("mid_price", 100.5);
        features.insert("spread", 0.5);

        assert_eq!(features.get("mid_price"), Some(100.5));
        assert!(features.contains("spread"));
        assert!(!features.contains("nonexistent"));
        assert_eq!(features.len(), 2);
    }
}
