//! Signal direction - core domain concept

use serde::{Deserialize, Serialize};

/// Signal direction representing trading intent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignalDirection {
    /// Buy signal - go long
    Buy,
    /// Sell signal - go short or close long
    Sell,
    /// No signal / neutral - no action
    #[default]
    None,
}

impl SignalDirection {
    /// Returns true if this direction requires action
    pub fn is_actionable(&self) -> bool {
        !matches!(self, SignalDirection::None)
    }

    /// Returns the opposite direction
    pub fn opposite(&self) -> Self {
        match self {
            SignalDirection::Buy => SignalDirection::Sell,
            SignalDirection::Sell => SignalDirection::Buy,
            SignalDirection::None => SignalDirection::None,
        }
    }

    /// Returns true if this is a buy signal
    pub fn is_buy(&self) -> bool {
        matches!(self, SignalDirection::Buy)
    }

    /// Returns true if this is a sell signal
    pub fn is_sell(&self) -> bool {
        matches!(self, SignalDirection::Sell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actionable() {
        assert!(SignalDirection::Buy.is_actionable());
        assert!(SignalDirection::Sell.is_actionable());
        assert!(!SignalDirection::None.is_actionable());
    }

    #[test]
    fn test_opposite() {
        assert_eq!(SignalDirection::Buy.opposite(), SignalDirection::Sell);
        assert_eq!(SignalDirection::Sell.opposite(), SignalDirection::Buy);
        assert_eq!(SignalDirection::None.opposite(), SignalDirection::None);
    }
}
