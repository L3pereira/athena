//! Signal Messages
//!
//! IPC message types for trading signals from strategy to execution.

use serde::{Deserialize, Serialize};

/// Signal message for IPC
///
/// Compact representation of a trading signal for inter-process communication.
/// Uses primitive types for efficient serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMessage {
    /// Unique signal identifier
    pub signal_id: String,
    /// Strategy that generated the signal
    pub strategy_id: String,
    /// Strategy type code
    pub strategy_type: u8,
    /// Exchange identifier
    pub exchange: String,
    /// Trading symbol
    pub symbol: String,
    /// Direction: -1 = sell, 0 = none/flat, 1 = buy
    pub direction: i8,
    /// Signal strength [-1.0, 1.0]
    pub strength: f32,
    /// Confidence [0.0, 1.0]
    pub confidence: f32,
    /// Current market price (raw i64)
    pub current_price_raw: i64,
    /// Estimated fair value (raw i64)
    pub fair_value_raw: i64,
    /// Suggested entry price (raw i64, 0 if not set)
    pub entry_price_raw: i64,
    /// Target price (raw i64, 0 if not set)
    pub target_price_raw: i64,
    /// Stop loss price (raw i64, 0 if not set)
    pub stop_price_raw: i64,
    /// Expected edge in basis points
    pub expected_edge_bps: i32,
    /// Signal generation timestamp (milliseconds since epoch)
    pub timestamp_ms: u64,
}

impl SignalMessage {
    /// Create a new signal message
    pub fn new(signal_id: &str, strategy_id: &str, exchange: &str, symbol: &str) -> Self {
        Self {
            signal_id: signal_id.to_string(),
            strategy_id: strategy_id.to_string(),
            strategy_type: 0,
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            direction: 0,
            strength: 0.0,
            confidence: 0.0,
            current_price_raw: 0,
            fair_value_raw: 0,
            entry_price_raw: 0,
            target_price_raw: 0,
            stop_price_raw: 0,
            expected_edge_bps: 0,
            timestamp_ms: current_timestamp_ms(),
        }
    }

    /// Set direction
    pub fn with_direction(mut self, direction: SignalDirection) -> Self {
        self.direction = direction.into();
        self
    }

    /// Set strength
    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength.clamp(-1.0, 1.0);
        self
    }

    /// Set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set prices (all raw i64)
    pub fn with_prices(
        mut self,
        current: i64,
        fair_value: i64,
        entry: i64,
        target: i64,
        stop: i64,
    ) -> Self {
        self.current_price_raw = current;
        self.fair_value_raw = fair_value;
        self.entry_price_raw = entry;
        self.target_price_raw = target;
        self.stop_price_raw = stop;
        self
    }

    /// Set expected edge in basis points
    pub fn with_edge_bps(mut self, edge_bps: i32) -> Self {
        self.expected_edge_bps = edge_bps;
        self
    }

    /// Get signal direction as enum
    pub fn direction(&self) -> SignalDirection {
        SignalDirection::from(self.direction)
    }

    /// Is this a buy signal?
    pub fn is_buy(&self) -> bool {
        self.direction > 0
    }

    /// Is this a sell signal?
    pub fn is_sell(&self) -> bool {
        self.direction < 0
    }

    /// Is this a neutral/flat signal?
    pub fn is_flat(&self) -> bool {
        self.direction == 0
    }
}

/// Signal direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDirection {
    /// Sell/short signal
    Sell = -1,
    /// No signal / flat
    None = 0,
    /// Buy/long signal
    Buy = 1,
}

impl From<i8> for SignalDirection {
    fn from(value: i8) -> Self {
        match value {
            v if v < 0 => SignalDirection::Sell,
            v if v > 0 => SignalDirection::Buy,
            _ => SignalDirection::None,
        }
    }
}

impl From<SignalDirection> for i8 {
    fn from(direction: SignalDirection) -> Self {
        direction as i8
    }
}

/// Get current timestamp in milliseconds
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Strategy type codes for IPC
///
/// These constants map to `StrategyType` enum values for efficient serialization.
/// Used in `SignalMessage::strategy_type` field.
#[allow(dead_code)]
pub mod strategy_types {
    /// Unknown or unspecified strategy
    pub const UNKNOWN: u8 = 0;
    /// Mean reversion strategies (OU, Kalman, z-score based)
    pub const MEAN_REVERSION: u8 = 1;
    /// Momentum / trend following strategies
    pub const MOMENTUM: u8 = 2;
    /// Statistical arbitrage (pairs, baskets)
    pub const STAT_ARB: u8 = 3;
    /// Latency arbitrage (cross-exchange)
    pub const LATENCY_ARB: u8 = 4;
    /// Triangular or multi-leg arbitrage
    pub const TRIANGULAR_ARB: u8 = 5;
    /// Market making strategies
    pub const MARKET_MAKING: u8 = 6;
    /// Order flow / microstructure strategies
    pub const ORDER_FLOW: u8 = 7;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_message() {
        let signal = SignalMessage::new("sig-1", "mean_reversion", "binance", "BTCUSDT")
            .with_direction(SignalDirection::Buy)
            .with_strength(0.8)
            .with_confidence(0.9)
            .with_edge_bps(50);

        assert!(signal.is_buy());
        assert!(!signal.is_sell());
        assert!(!signal.is_flat());
        assert_eq!(signal.strength, 0.8);
        assert_eq!(signal.confidence, 0.9);
        assert_eq!(signal.expected_edge_bps, 50);
    }

    #[test]
    fn test_signal_direction_conversion() {
        assert_eq!(SignalDirection::from(-5i8), SignalDirection::Sell);
        assert_eq!(SignalDirection::from(0i8), SignalDirection::None);
        assert_eq!(SignalDirection::from(3i8), SignalDirection::Buy);

        assert_eq!(i8::from(SignalDirection::Sell), -1);
        assert_eq!(i8::from(SignalDirection::None), 0);
        assert_eq!(i8::from(SignalDirection::Buy), 1);
    }

    #[test]
    fn test_signal_strength_clamping() {
        let signal = SignalMessage::new("sig-1", "test", "test", "TEST")
            .with_strength(5.0)
            .with_confidence(2.0);

        assert_eq!(signal.strength, 1.0); // Clamped to max
        assert_eq!(signal.confidence, 1.0); // Clamped to max

        let signal2 = SignalMessage::new("sig-2", "test", "test", "TEST")
            .with_strength(-5.0)
            .with_confidence(-1.0);

        assert_eq!(signal2.strength, -1.0); // Clamped to min
        assert_eq!(signal2.confidence, 0.0); // Clamped to min
    }

    #[test]
    fn test_strategy_type_codes() {
        // Verify strategy type codes are contiguous and ordered
        assert_eq!(strategy_types::UNKNOWN, 0);
        assert_eq!(strategy_types::MEAN_REVERSION, 1);
        assert_eq!(strategy_types::MOMENTUM, 2);
        assert_eq!(strategy_types::STAT_ARB, 3);
        assert_eq!(strategy_types::LATENCY_ARB, 4);
        assert_eq!(strategy_types::TRIANGULAR_ARB, 5);
        assert_eq!(strategy_types::MARKET_MAKING, 6);
        assert_eq!(strategy_types::ORDER_FLOW, 7);

        // Use in signal message
        let mut signal = SignalMessage::new("sig-1", "mr_btc", "binance", "BTCUSDT");
        signal.strategy_type = strategy_types::MEAN_REVERSION;
        assert_eq!(signal.strategy_type, 1);
    }
}
