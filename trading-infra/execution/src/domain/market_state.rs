//! Market State Types
//!
//! Domain models for representing current market conditions.

use serde::{Deserialize, Serialize};
use trading_core::{Price, PriceLevel, Quantity};

/// Snapshot of current market state for impact/execution calculations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketState {
    /// Best bid level
    pub best_bid: Option<PriceLevel>,
    /// Best ask level
    pub best_ask: Option<PriceLevel>,
    /// Mid price
    pub mid_price: Price,
    /// Current spread
    pub spread: Price,
    /// Total visible bid depth (e.g., top 10 levels)
    pub bid_depth: Quantity,
    /// Total visible ask depth (e.g., top 10 levels)
    pub ask_depth: Quantity,
    /// Recent volatility estimate (annualized, 0.0-1.0)
    pub volatility: f64,
    /// Average daily volume for this instrument
    pub daily_volume: Quantity,
}

impl MarketState {
    /// Get the imbalance ratio: (bids - asks) / (bids + asks)
    /// Returns value in [-1.0, 1.0]
    /// Positive = more bid depth, Negative = more ask depth
    pub fn imbalance(&self) -> f64 {
        let total = self.bid_depth.raw() + self.ask_depth.raw();
        if total == 0 {
            return 0.0;
        }
        let bid = self.bid_depth.raw() as f64;
        let ask = self.ask_depth.raw() as f64;
        (bid - ask) / (bid + ask)
    }

    /// Get spread in basis points relative to mid
    pub fn spread_bps(&self) -> f64 {
        if self.mid_price.is_zero() {
            return 0.0;
        }
        (self.spread.raw() as f64 / self.mid_price.raw() as f64) * 10_000.0
    }

    /// Check if market is crossed (invalid state)
    pub fn is_crossed(&self) -> bool {
        match (&self.best_bid, &self.best_ask) {
            (Some(bid), Some(ask)) => bid.price >= ask.price,
            _ => false,
        }
    }

    /// Total depth on both sides
    pub fn total_depth(&self) -> Quantity {
        self.bid_depth + self.ask_depth
    }
}

impl Default for MarketState {
    fn default() -> Self {
        Self {
            best_bid: None,
            best_ask: None,
            mid_price: Price::ZERO,
            spread: Price::ZERO,
            bid_depth: Quantity::ZERO,
            ask_depth: Quantity::ZERO,
            volatility: 0.0,
            daily_volume: Quantity::ZERO,
        }
    }
}

/// Real-time market conditions for adaptive execution
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MarketConditions {
    /// Current spread relative to normal (1.0 = normal)
    pub spread_ratio: f64,
    /// Current depth relative to normal (1.0 = normal)
    pub depth_ratio: f64,
    /// Current volatility relative to normal (1.0 = normal)
    pub volatility_ratio: f64,
    /// Order flow imbalance [-1.0, 1.0]
    pub flow_imbalance: f64,
    /// Fill rate (what % of orders are filling)
    pub fill_rate: f64,
}

impl MarketConditions {
    /// Assess overall market quality (1.0 = excellent, 0.0 = poor)
    pub fn quality_score(&self) -> f64 {
        // Lower spread = better
        let spread_score = (1.0 / self.spread_ratio).min(1.0);
        // Higher depth = better
        let depth_score = self.depth_ratio.min(1.0);
        // Lower volatility = better for execution
        let vol_score = (1.0 / self.volatility_ratio).min(1.0);
        // Higher fill rate = better
        let fill_score = self.fill_rate;

        // Weighted average
        (spread_score * 0.3 + depth_score * 0.3 + vol_score * 0.2 + fill_score * 0.2)
            .clamp(0.0, 1.0)
    }

    /// Check if conditions are favorable for aggressive execution
    pub fn is_favorable(&self) -> bool {
        self.quality_score() >= 0.7
    }

    /// Check if conditions are adverse
    pub fn is_adverse(&self) -> bool {
        self.quality_score() < 0.3
    }
}

impl Default for MarketConditions {
    fn default() -> Self {
        Self {
            spread_ratio: 1.0,
            depth_ratio: 1.0,
            volatility_ratio: 1.0,
            flow_imbalance: 0.0,
            fill_rate: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_state_imbalance() {
        let state = MarketState {
            bid_depth: Quantity::from_int(1000),
            ask_depth: Quantity::from_int(1000),
            ..Default::default()
        };
        assert!(state.imbalance().abs() < 0.001);

        let bid_heavy = MarketState {
            bid_depth: Quantity::from_int(2000),
            ask_depth: Quantity::from_int(1000),
            ..Default::default()
        };
        assert!((bid_heavy.imbalance() - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_spread_bps() {
        let state = MarketState {
            mid_price: Price::from_int(100),
            spread: Price::from_raw(10_000_000), // 0.1
            ..Default::default()
        };
        assert!((state.spread_bps() - 10.0).abs() < 0.1); // 10 bps
    }

    #[test]
    fn test_conditions_quality() {
        let excellent = MarketConditions {
            spread_ratio: 0.5,
            depth_ratio: 1.5,
            volatility_ratio: 0.5,
            flow_imbalance: 0.0,
            fill_rate: 0.9,
        };
        assert!(excellent.is_favorable());

        let poor = MarketConditions {
            spread_ratio: 3.0,
            depth_ratio: 0.2,
            volatility_ratio: 3.0,
            flow_imbalance: 0.8,
            fill_rate: 0.1,
        };
        assert!(poor.is_adverse());
    }
}
