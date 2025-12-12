//! Strategy domain concepts - identifiers and classifications

use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a strategy
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrategyId(String);

impl StrategyId {
    /// Create a new strategy identifier
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the underlying string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StrategyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for StrategyId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for StrategyId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Strategy type classification
///
/// Categorizes strategies by their trading approach.
/// Used for risk management and signal routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyType {
    /// Mean reversion strategies (OU, Kalman, z-score based)
    MeanReversion,
    /// Momentum / trend following strategies
    Momentum,
    /// Statistical arbitrage (pairs, baskets)
    StatArb,
    /// Latency arbitrage (cross-exchange)
    LatencyArb,
    /// Triangular or multi-leg arbitrage
    TriangularArb,
    /// Market making strategies
    MarketMaking,
    /// Order flow / microstructure strategies
    OrderFlow,
}

impl StrategyType {
    /// Returns true if this strategy type typically requires fast execution
    pub fn is_latency_sensitive(&self) -> bool {
        matches!(
            self,
            StrategyType::LatencyArb | StrategyType::TriangularArb | StrategyType::MarketMaking
        )
    }

    /// Returns true if this is an arbitrage strategy
    pub fn is_arbitrage(&self) -> bool {
        matches!(
            self,
            StrategyType::StatArb | StrategyType::LatencyArb | StrategyType::TriangularArb
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_id() {
        let id = StrategyId::new("mean_reversion_btc");
        assert_eq!(id.as_str(), "mean_reversion_btc");
        assert_eq!(id.to_string(), "mean_reversion_btc");
    }

    #[test]
    fn test_strategy_type_classification() {
        assert!(StrategyType::LatencyArb.is_latency_sensitive());
        assert!(StrategyType::MarketMaking.is_latency_sensitive());
        assert!(!StrategyType::MeanReversion.is_latency_sensitive());

        assert!(StrategyType::StatArb.is_arbitrage());
        assert!(!StrategyType::Momentum.is_arbitrage());
    }
}
