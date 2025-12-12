//! Signal entity - the core output of signal generation

use super::{Features, Leg, SignalDirection, StrategyId, StrategyType, Urgency};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use trading_core::Price;
use uuid::Uuid;

/// Unique signal identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignalId(String);

impl SignalId {
    /// Generate a new unique signal ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from existing string
    pub fn from_string(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the underlying string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SignalId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SignalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Trading signal - the primary output entity
///
/// Represents a trading recommendation from a strategy.
/// This is an immutable value object once created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    // === Identification ===
    /// When signal was generated (unix timestamp ms)
    pub timestamp_ms: u64,
    /// Unique signal identifier
    pub signal_id: SignalId,
    /// Which strategy generated this
    pub strategy_id: StrategyId,
    /// Strategy classification
    pub strategy_type: StrategyType,

    // === Core Signal ===
    /// Primary instrument symbol
    pub symbol: String,
    /// Signal direction
    pub direction: SignalDirection,
    /// Signal strength for position sizing [-1, 1]
    pub strength: f64,
    /// Model confidence [0, 1]
    pub confidence: f64,
    /// How fast edge decays [0, 1]
    pub urgency: Urgency,

    // === Prices ===
    /// Observed price when signal generated
    pub current_price: Price,
    /// Model's fair value estimate
    pub fair_value: Price,
    /// Suggested entry price (for limit orders)
    pub entry_price: Option<Price>,
    /// Expected exit / target price
    pub target_price: Option<Price>,
    /// Stop loss level
    pub stop_price: Option<Price>,

    // === Multi-leg ===
    /// Legs for multi-leg strategies (empty for single-leg)
    pub legs: Vec<Leg>,

    // === Risk Metrics ===
    /// Expected profit in basis points
    pub expected_edge_bps: Option<f64>,
    /// For mean reversion: expected time to revert (seconds)
    pub half_life_seconds: Option<f64>,
    /// Uncertainty in fair value estimate
    pub model_variance: Option<f64>,

    // === Metadata ===
    /// Key features that drove the signal
    pub features: HashMap<String, f64>,
    /// Model version for tracking
    pub model_version: String,
}

impl Signal {
    /// Create a new signal builder
    pub fn builder(
        strategy_id: StrategyId,
        strategy_type: StrategyType,
        symbol: impl Into<String>,
    ) -> SignalBuilder {
        SignalBuilder::new(strategy_id, strategy_type, symbol)
    }

    /// Check if signal is actionable (not None direction)
    pub fn is_actionable(&self) -> bool {
        self.direction.is_actionable()
    }

    /// Check if this is a buy signal
    pub fn is_buy(&self) -> bool {
        self.direction.is_buy()
    }

    /// Check if this is a sell signal
    pub fn is_sell(&self) -> bool {
        self.direction.is_sell()
    }

    /// Get signal age in milliseconds
    pub fn age_ms(&self) -> u64 {
        current_timestamp_ms().saturating_sub(self.timestamp_ms)
    }

    /// Check if signal is stale (older than given milliseconds)
    pub fn is_stale(&self, max_age_ms: u64) -> bool {
        self.age_ms() > max_age_ms
    }

    /// Check if this is a multi-leg signal
    pub fn is_multi_leg(&self) -> bool {
        !self.legs.is_empty()
    }
}

/// Builder for constructing Signal entities
pub struct SignalBuilder {
    strategy_id: StrategyId,
    strategy_type: StrategyType,
    symbol: String,
    direction: SignalDirection,
    strength: f64,
    confidence: f64,
    urgency: Urgency,
    current_price: Price,
    fair_value: Price,
    entry_price: Option<Price>,
    target_price: Option<Price>,
    stop_price: Option<Price>,
    legs: Vec<Leg>,
    expected_edge_bps: Option<f64>,
    half_life_seconds: Option<f64>,
    model_variance: Option<f64>,
    features: HashMap<String, f64>,
    model_version: String,
}

impl SignalBuilder {
    /// Create a new builder with required fields
    pub fn new(
        strategy_id: StrategyId,
        strategy_type: StrategyType,
        symbol: impl Into<String>,
    ) -> Self {
        Self {
            strategy_id,
            strategy_type,
            symbol: symbol.into(),
            direction: SignalDirection::None,
            strength: 0.0,
            confidence: 0.0,
            urgency: Urgency::default(),
            current_price: Price::ZERO,
            fair_value: Price::ZERO,
            entry_price: None,
            target_price: None,
            stop_price: None,
            legs: Vec::new(),
            expected_edge_bps: None,
            half_life_seconds: None,
            model_variance: None,
            features: HashMap::new(),
            model_version: "1.0.0".to_string(),
        }
    }

    /// Set the direction
    pub fn direction(mut self, direction: SignalDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Set direction to buy
    pub fn buy(mut self) -> Self {
        self.direction = SignalDirection::Buy;
        self
    }

    /// Set direction to sell
    pub fn sell(mut self) -> Self {
        self.direction = SignalDirection::Sell;
        self
    }

    /// Set signal strength (clamped to [-1, 1])
    pub fn strength(mut self, strength: f64) -> Self {
        self.strength = strength.clamp(-1.0, 1.0);
        self
    }

    /// Set confidence (clamped to [0, 1])
    pub fn confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set urgency
    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    /// Set current and fair value prices
    pub fn prices(mut self, current: Price, fair_value: Price) -> Self {
        self.current_price = current;
        self.fair_value = fair_value;
        self
    }

    /// Set entry price
    pub fn entry_price(mut self, price: Price) -> Self {
        self.entry_price = Some(price);
        self
    }

    /// Set target price
    pub fn target_price(mut self, price: Price) -> Self {
        self.target_price = Some(price);
        self
    }

    /// Set stop price
    pub fn stop_price(mut self, price: Price) -> Self {
        self.stop_price = Some(price);
        self
    }

    /// Add a leg
    pub fn leg(mut self, leg: Leg) -> Self {
        self.legs.push(leg);
        self
    }

    /// Set all legs
    pub fn legs(mut self, legs: Vec<Leg>) -> Self {
        self.legs = legs;
        self
    }

    /// Set expected edge in basis points
    pub fn expected_edge_bps(mut self, bps: f64) -> Self {
        self.expected_edge_bps = Some(bps);
        self
    }

    /// Set half-life in seconds
    pub fn half_life_seconds(mut self, seconds: f64) -> Self {
        self.half_life_seconds = Some(seconds);
        self
    }

    /// Set model variance
    pub fn model_variance(mut self, variance: f64) -> Self {
        self.model_variance = Some(variance);
        self
    }

    /// Add a single feature
    pub fn feature(mut self, name: impl Into<String>, value: f64) -> Self {
        self.features.insert(name.into(), value);
        self
    }

    /// Set all features
    pub fn features(mut self, features: HashMap<String, f64>) -> Self {
        self.features = features;
        self
    }

    /// Set features from Features type
    pub fn with_features(mut self, features: Features) -> Self {
        self.features = features.into_map();
        self
    }

    /// Set model version
    pub fn model_version(mut self, version: impl Into<String>) -> Self {
        self.model_version = version.into();
        self
    }

    /// Build the signal
    pub fn build(self) -> Signal {
        Signal {
            timestamp_ms: current_timestamp_ms(),
            signal_id: SignalId::new(),
            strategy_id: self.strategy_id,
            strategy_type: self.strategy_type,
            symbol: self.symbol,
            direction: self.direction,
            strength: self.strength,
            confidence: self.confidence,
            urgency: self.urgency,
            current_price: self.current_price,
            fair_value: self.fair_value,
            entry_price: self.entry_price,
            target_price: self.target_price,
            stop_price: self.stop_price,
            legs: self.legs,
            expected_edge_bps: self.expected_edge_bps,
            half_life_seconds: self.half_life_seconds,
            model_variance: self.model_variance,
            features: self.features,
            model_version: self.model_version,
        }
    }
}

/// Get current timestamp in milliseconds
fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_builder() {
        let signal = Signal::builder(
            StrategyId::new("test"),
            StrategyType::MeanReversion,
            "BTCUSDT",
        )
        .buy()
        .strength(0.75)
        .confidence(0.85)
        .urgency(Urgency::medium())
        .prices(Price::from_int(50000), Price::from_int(50500))
        .expected_edge_bps(25.0)
        .feature("z_score", -2.1)
        .build();

        assert_eq!(signal.symbol, "BTCUSDT");
        assert!(signal.is_buy());
        assert!(signal.is_actionable());
        assert_eq!(signal.strength, 0.75);
        assert_eq!(signal.features.get("z_score"), Some(&-2.1));
    }

    #[test]
    fn test_signal_id() {
        let id1 = SignalId::new();
        let id2 = SignalId::new();
        assert_ne!(id1, id2); // UUIDs should be unique
    }

    #[test]
    fn test_signal_age() {
        let signal = Signal::builder(
            StrategyId::new("test"),
            StrategyType::MeanReversion,
            "BTCUSDT",
        )
        .build();

        // Should be very recent
        assert!(signal.age_ms() < 100);
        assert!(!signal.is_stale(1000));
    }
}
