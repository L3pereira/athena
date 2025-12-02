//! Trading Risk Parameters
//!
//! These are the risk limits published to the Order Manager.
//! The OM uses these to validate that execution plans fit within bounds.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Risk parameters published to Order Manager
///
/// This is the "contract" between Risk Manager and Order Manager.
/// OM uses these to validate execution decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingRiskParameters {
    /// Is trading enabled? (master kill switch)
    pub trading_enabled: bool,
    /// Reason if disabled
    pub disabled_reason: Option<String>,
    /// Per-instrument limits
    pub instrument_limits: HashMap<String, InstrumentLimits>,
    /// Per-strategy limits
    pub strategy_limits: HashMap<String, StrategyLimits>,
    /// Drawdown limits
    pub drawdown: DrawdownLimits,
    /// Execution cost limits
    pub cost: CostLimits,
    /// Market quality by instrument
    pub market_quality: HashMap<String, MarketQualityLimit>,
    /// When these parameters were computed
    pub timestamp: DateTime<Utc>,
    /// Version (for OM to detect updates)
    pub version: u64,
}

impl Default for TradingRiskParameters {
    fn default() -> Self {
        Self {
            trading_enabled: true,
            disabled_reason: None,
            instrument_limits: HashMap::new(),
            strategy_limits: HashMap::new(),
            drawdown: DrawdownLimits::default(),
            cost: CostLimits::default(),
            market_quality: HashMap::new(),
            timestamp: Utc::now(),
            version: 0,
        }
    }
}

impl TradingRiskParameters {
    /// Check if trading is allowed for an instrument
    pub fn can_trade(&self, instrument_id: &str) -> bool {
        if !self.trading_enabled {
            return false;
        }

        // Check market quality
        if let Some(mq) = self.market_quality.get(instrument_id) {
            if !mq.tradeable {
                return false;
            }
        }

        true
    }

    /// Get position limit for instrument
    pub fn position_limit(&self, instrument_id: &str) -> Decimal {
        self.instrument_limits
            .get(instrument_id)
            .map(|l| l.max_position)
            .unwrap_or(dec!(100)) // Default limit
    }

    /// Get max order size for instrument
    pub fn max_order_size(&self, instrument_id: &str) -> Decimal {
        self.instrument_limits
            .get(instrument_id)
            .map(|l| l.max_order_size)
            .unwrap_or(dec!(10)) // Default
    }

    /// Check if cost is acceptable given expected alpha
    pub fn is_cost_acceptable(
        &self,
        estimated_cost_bps: Decimal,
        expected_alpha_bps: Decimal,
    ) -> bool {
        if expected_alpha_bps.is_zero() {
            return estimated_cost_bps <= self.cost.max_cost_bps;
        }
        let cost_ratio = estimated_cost_bps / expected_alpha_bps;
        cost_ratio <= self.cost.max_cost_to_alpha_ratio
    }
}

/// Limits for a specific instrument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentLimits {
    /// Maximum position size (absolute)
    pub max_position: Decimal,
    /// Maximum order size per order
    pub max_order_size: Decimal,
    /// Maximum notional exposure
    pub max_notional: Decimal,
    /// Minimum time between orders (rate limiting)
    pub min_order_interval_ms: u64,
    /// Is this instrument enabled for trading?
    pub enabled: bool,
}

impl Default for InstrumentLimits {
    fn default() -> Self {
        Self {
            max_position: dec!(100),
            max_order_size: dec!(10),
            max_notional: dec!(1_000_000),
            min_order_interval_ms: 0,
            enabled: true,
        }
    }
}

/// Limits for a specific strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyLimits {
    /// Strategy ID
    pub strategy_id: String,
    /// Maximum position across all instruments
    pub max_total_position: Decimal,
    /// Maximum notional exposure
    pub max_notional: Decimal,
    /// Maximum daily loss allowed
    pub max_daily_loss: Decimal,
    /// Maximum drawdown from peak
    pub max_drawdown_pct: Decimal,
    /// Weight in portfolio (0.0 - 1.0)
    pub portfolio_weight: Decimal,
    /// Is this strategy enabled?
    pub enabled: bool,
}

impl Default for StrategyLimits {
    fn default() -> Self {
        Self {
            strategy_id: String::new(),
            max_total_position: dec!(100),
            max_notional: dec!(500_000),
            max_daily_loss: dec!(10_000),
            max_drawdown_pct: dec!(0.10),
            portfolio_weight: dec!(1.0),
            enabled: true,
        }
    }
}

/// Portfolio-wide drawdown limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawdownLimits {
    /// Maximum daily loss before halting
    pub max_daily_loss: Decimal,
    /// Warning threshold (reduce size)
    pub daily_loss_warning: Decimal,
    /// Maximum drawdown from peak
    pub max_drawdown_pct: Decimal,
    /// Warning threshold
    pub drawdown_warning_pct: Decimal,
    /// Current daily PnL (updated by Risk Manager)
    pub current_daily_pnl: Decimal,
    /// Current drawdown from peak
    pub current_drawdown_pct: Decimal,
    /// Has daily limit been breached?
    pub daily_limit_breached: bool,
    /// Has drawdown limit been breached?
    pub drawdown_limit_breached: bool,
}

impl Default for DrawdownLimits {
    fn default() -> Self {
        Self {
            max_daily_loss: dec!(50_000),
            daily_loss_warning: dec!(25_000),
            max_drawdown_pct: dec!(0.10),
            drawdown_warning_pct: dec!(0.05),
            current_daily_pnl: Decimal::ZERO,
            current_drawdown_pct: Decimal::ZERO,
            daily_limit_breached: false,
            drawdown_limit_breached: false,
        }
    }
}

impl DrawdownLimits {
    /// Should trading be halted?
    pub fn should_halt(&self) -> bool {
        self.daily_limit_breached || self.drawdown_limit_breached
    }

    /// Should position sizes be reduced?
    pub fn should_reduce_size(&self) -> bool {
        self.current_daily_pnl < -self.daily_loss_warning
            || self.current_drawdown_pct > self.drawdown_warning_pct
    }

    /// Size multiplier based on current drawdown (1.0 = full, 0.0 = none)
    pub fn size_multiplier(&self) -> Decimal {
        if self.should_halt() {
            return Decimal::ZERO;
        }

        if self.should_reduce_size() {
            // Linear reduction from 1.0 at warning to 0.0 at limit
            let daily_ratio = if self.max_daily_loss > self.daily_loss_warning {
                let range = self.max_daily_loss - self.daily_loss_warning;
                let excess = (-self.current_daily_pnl - self.daily_loss_warning).max(Decimal::ZERO);
                (Decimal::ONE - excess / range).max(Decimal::ZERO)
            } else {
                Decimal::ONE
            };

            let dd_ratio = if self.max_drawdown_pct > self.drawdown_warning_pct {
                let range = self.max_drawdown_pct - self.drawdown_warning_pct;
                let excess =
                    (self.current_drawdown_pct - self.drawdown_warning_pct).max(Decimal::ZERO);
                (Decimal::ONE - excess / range).max(Decimal::ZERO)
            } else {
                Decimal::ONE
            };

            daily_ratio.min(dd_ratio)
        } else {
            Decimal::ONE
        }
    }
}

/// Execution cost limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostLimits {
    /// Maximum acceptable cost in basis points
    pub max_cost_bps: Decimal,
    /// Maximum cost as ratio of expected alpha
    pub max_cost_to_alpha_ratio: Decimal,
    /// Minimum alpha required to trade (in bps)
    pub min_alpha_bps: Decimal,
}

impl Default for CostLimits {
    fn default() -> Self {
        Self {
            max_cost_bps: dec!(50),             // 50 bps max cost
            max_cost_to_alpha_ratio: dec!(0.5), // Cost can't exceed 50% of alpha
            min_alpha_bps: dec!(5),             // Need at least 5 bps alpha to trade
        }
    }
}

/// Market quality limit for an instrument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketQualityLimit {
    /// Is the market tradeable?
    pub tradeable: bool,
    /// Reason if not tradeable
    pub reason: Option<String>,
    /// Quality score (0.0 - 1.0)
    pub quality_score: Decimal,
    /// Spread is acceptable?
    pub spread_acceptable: bool,
    /// Book depth is acceptable?
    pub depth_acceptable: bool,
    /// Any manipulation detected?
    pub manipulation_detected: bool,
}

impl Default for MarketQualityLimit {
    fn default() -> Self {
        Self {
            tradeable: true,
            reason: None,
            quality_score: Decimal::ONE,
            spread_acceptable: true,
            depth_acceptable: true,
            manipulation_detected: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_parameters() {
        let params = TradingRiskParameters::default();
        assert!(params.trading_enabled);
        assert!(params.can_trade("BTC-USD"));
    }

    #[test]
    fn test_trading_disabled() {
        let mut params = TradingRiskParameters::default();
        params.trading_enabled = false;
        params.disabled_reason = Some("Emergency halt".to_string());

        assert!(!params.can_trade("BTC-USD"));
    }

    #[test]
    fn test_market_quality_blocks_trading() {
        let mut params = TradingRiskParameters::default();
        params.market_quality.insert(
            "BTC-USD".to_string(),
            MarketQualityLimit {
                tradeable: false,
                reason: Some("Manipulation detected".to_string()),
                ..Default::default()
            },
        );

        assert!(!params.can_trade("BTC-USD"));
        assert!(params.can_trade("ETH-USD")); // Other instruments still OK
    }

    #[test]
    fn test_cost_acceptable() {
        let params = TradingRiskParameters::default();

        // Cost 10bps, alpha 50bps -> ratio 0.2 -> OK
        assert!(params.is_cost_acceptable(dec!(10), dec!(50)));

        // Cost 30bps, alpha 50bps -> ratio 0.6 -> NOT OK (> 0.5)
        assert!(!params.is_cost_acceptable(dec!(30), dec!(50)));

        // Cost 40bps, alpha 0 -> check max_cost_bps
        assert!(params.is_cost_acceptable(dec!(40), dec!(0))); // 40 < 50
        assert!(!params.is_cost_acceptable(dec!(60), dec!(0))); // 60 > 50
    }

    #[test]
    fn test_drawdown_size_multiplier() {
        let mut dd = DrawdownLimits::default();

        // No drawdown -> full size
        assert_eq!(dd.size_multiplier(), Decimal::ONE);

        // Past warning threshold -> starts reducing
        // Warning is 25_000, so -30_000 should trigger reduction
        dd.current_daily_pnl = -dec!(30_000);
        assert!(dd.size_multiplier() < Decimal::ONE);
        assert!(dd.size_multiplier() > Decimal::ZERO);

        // At limit -> zero
        dd.daily_limit_breached = true;
        assert_eq!(dd.size_multiplier(), Decimal::ZERO);
    }
}
