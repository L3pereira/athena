//! Market Regime Types
//!
//! Represents different market states and transitions between them.

use serde::{Deserialize, Serialize};

/// Market regime classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketRegime {
    /// Normal market conditions
    Normal,
    /// Volatile but orderly
    Volatile,
    /// Strong directional trend
    Trending,
    /// Low liquidity, wide spreads
    Stressed,
    /// Extreme conditions, potential crisis
    Crisis,
}

impl MarketRegime {
    /// Get recommended spread multiplier for this regime
    pub fn spread_multiplier(&self) -> f64 {
        match self {
            MarketRegime::Normal => 1.0,
            MarketRegime::Volatile => 1.5,
            MarketRegime::Trending => 1.3,
            MarketRegime::Stressed => 2.5,
            MarketRegime::Crisis => 5.0,
        }
    }

    /// Get recommended depth multiplier for this regime
    pub fn depth_multiplier(&self) -> f64 {
        match self {
            MarketRegime::Normal => 1.0,
            MarketRegime::Volatile => 0.7,
            MarketRegime::Trending => 0.8,
            MarketRegime::Stressed => 0.3,
            MarketRegime::Crisis => 0.1,
        }
    }

    /// Check if this regime is adverse for market making
    pub fn is_adverse(&self) -> bool {
        matches!(self, MarketRegime::Stressed | MarketRegime::Crisis)
    }

    /// Check if quotes should be pulled
    pub fn should_pull_quotes(&self) -> bool {
        matches!(self, MarketRegime::Crisis)
    }
}

impl Default for MarketRegime {
    fn default() -> Self {
        MarketRegime::Normal
    }
}

/// Regime shift event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegimeShift {
    /// Previous regime
    pub from: MarketRegime,
    /// New regime
    pub to: MarketRegime,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f64,
    /// Trigger description
    pub trigger: String,
    /// Number of steps the deviation persisted
    pub persistence_steps: u32,
}

impl RegimeShift {
    pub fn new(
        from: MarketRegime,
        to: MarketRegime,
        confidence: f64,
        trigger: impl Into<String>,
    ) -> Self {
        Self {
            from,
            to,
            confidence,
            trigger: trigger.into(),
            persistence_steps: 0,
        }
    }

    /// Check if this is a shift to an adverse regime
    pub fn is_deteriorating(&self) -> bool {
        !self.from.is_adverse() && self.to.is_adverse()
    }

    /// Check if this is a recovery from adverse conditions
    pub fn is_recovering(&self) -> bool {
        self.from.is_adverse() && !self.to.is_adverse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regime_multipliers() {
        assert!((MarketRegime::Normal.spread_multiplier() - 1.0).abs() < 0.01);
        assert!(MarketRegime::Crisis.spread_multiplier() > 4.0);
        assert!(MarketRegime::Crisis.depth_multiplier() < 0.2);
    }

    #[test]
    fn test_adverse_regimes() {
        assert!(!MarketRegime::Normal.is_adverse());
        assert!(!MarketRegime::Volatile.is_adverse());
        assert!(MarketRegime::Stressed.is_adverse());
        assert!(MarketRegime::Crisis.is_adverse());
    }

    #[test]
    fn test_regime_shift() {
        let shift = RegimeShift::new(
            MarketRegime::Normal,
            MarketRegime::Stressed,
            0.85,
            "Spread widened 3x",
        );
        assert!(shift.is_deteriorating());
        assert!(!shift.is_recovering());
    }
}
