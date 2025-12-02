//! Signal - What strategies output
//!
//! Strategies don't output orders directly. They output signals that express
//! their desired position and conviction. The Order Manager aggregates these
//! signals across strategies to build the target portfolio.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// How urgent is this signal?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Urgency {
    /// Post limit orders, be patient
    Passive,
    /// Normal execution (default)
    #[default]
    Normal,
    /// Need to fill quickly, willing to cross spread
    Aggressive,
    /// Emergency - market order immediately
    Immediate,
}

/// Signal from a strategy
///
/// Expresses the strategy's desired position for an instrument,
/// along with conviction metrics that help with aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Which strategy generated this signal
    pub strategy_id: String,
    /// Instrument to trade
    pub instrument_id: String,
    /// Target position (not delta!)
    /// Positive = long, Negative = short, Zero = flat
    pub target_position: Decimal,
    /// Expected return (alpha) for this position
    /// Used for portfolio optimization and allocation
    pub alpha: Option<Decimal>,
    /// Confidence in the signal (0.0 - 1.0)
    /// Higher confidence = larger allocation
    pub confidence: Decimal,
    /// How urgent is this signal?
    pub urgency: Urgency,
    /// Optional: Maximum position size strategy allows
    pub max_position: Option<Decimal>,
    /// Optional: Stop loss price
    pub stop_loss: Option<Decimal>,
    /// Optional: Take profit price
    pub take_profit: Option<Decimal>,
    /// When the signal was generated
    pub timestamp: DateTime<Utc>,
    /// Optional: Signal expires at this time
    pub expires_at: Option<DateTime<Utc>>,
}

impl Signal {
    /// Create a new signal
    pub fn new(
        strategy_id: impl Into<String>,
        instrument_id: impl Into<String>,
        target_position: Decimal,
    ) -> Self {
        Self {
            strategy_id: strategy_id.into(),
            instrument_id: instrument_id.into(),
            target_position,
            alpha: None,
            confidence: Decimal::ONE,
            urgency: Urgency::Normal,
            max_position: None,
            stop_loss: None,
            take_profit: None,
            timestamp: Utc::now(),
            expires_at: None,
        }
    }

    /// Create a "go flat" signal
    pub fn flatten(strategy_id: impl Into<String>, instrument_id: impl Into<String>) -> Self {
        Self::new(strategy_id, instrument_id, Decimal::ZERO).with_urgency(Urgency::Aggressive)
    }

    /// Builder: Set alpha (expected return)
    pub fn with_alpha(mut self, alpha: Decimal) -> Self {
        self.alpha = Some(alpha);
        self
    }

    /// Builder: Set confidence
    pub fn with_confidence(mut self, confidence: Decimal) -> Self {
        self.confidence = confidence.clamp(Decimal::ZERO, Decimal::ONE);
        self
    }

    /// Builder: Set urgency
    pub fn with_urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    /// Builder: Set max position
    pub fn with_max_position(mut self, max: Decimal) -> Self {
        self.max_position = Some(max);
        self
    }

    /// Builder: Set stop loss
    pub fn with_stop_loss(mut self, price: Decimal) -> Self {
        self.stop_loss = Some(price);
        self
    }

    /// Builder: Set take profit
    pub fn with_take_profit(mut self, price: Decimal) -> Self {
        self.take_profit = Some(price);
        self
    }

    /// Builder: Set expiry
    pub fn with_expiry(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Check if signal is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }

    /// Is this a long signal?
    pub fn is_long(&self) -> bool {
        self.target_position > Decimal::ZERO
    }

    /// Is this a short signal?
    pub fn is_short(&self) -> bool {
        self.target_position < Decimal::ZERO
    }

    /// Is this a flatten signal?
    pub fn is_flatten(&self) -> bool {
        self.target_position.is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_signal_creation() {
        let signal = Signal::new("mm-btc", "BTC-USD", dec!(1.5))
            .with_alpha(dec!(0.02))
            .with_confidence(dec!(0.8))
            .with_urgency(Urgency::Passive);

        assert_eq!(signal.strategy_id, "mm-btc");
        assert_eq!(signal.instrument_id, "BTC-USD");
        assert_eq!(signal.target_position, dec!(1.5));
        assert_eq!(signal.alpha, Some(dec!(0.02)));
        assert_eq!(signal.confidence, dec!(0.8));
        assert!(signal.is_long());
    }

    #[test]
    fn test_flatten_signal() {
        let signal = Signal::flatten("mm-btc", "BTC-USD");

        assert!(signal.is_flatten());
        assert_eq!(signal.urgency, Urgency::Aggressive);
    }

    #[test]
    fn test_confidence_clamping() {
        let signal = Signal::new("test", "BTC-USD", dec!(1.0)).with_confidence(dec!(1.5)); // Should clamp to 1.0

        assert_eq!(signal.confidence, Decimal::ONE);
    }
}
