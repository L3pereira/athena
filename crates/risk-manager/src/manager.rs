//! Trading Risk Manager
//!
//! Active risk management that:
//! - Monitors PnL and drawdown
//! - Updates risk parameters based on market conditions
//! - Publishes parameters to Order Manager
//! - Can halt trading on limit breaches

use crate::parameters::{
    CostLimits, DrawdownLimits, InstrumentLimits, MarketQualityLimit, StrategyLimits,
    TradingRiskParameters,
};
use crate::surveillance::{BasicSurveillance, MarketSurveillance, SurveillanceConfig};
use chrono::Utc;
use log::{error, info};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

/// Configuration for the Trading Risk Manager
#[derive(Debug, Clone)]
pub struct TradingRiskConfig {
    /// Default instrument limits
    pub default_instrument_limits: InstrumentLimits,
    /// Default strategy limits
    pub default_strategy_limits: StrategyLimits,
    /// Drawdown limits
    pub drawdown_limits: DrawdownLimits,
    /// Cost limits
    pub cost_limits: CostLimits,
    /// Surveillance configuration
    pub surveillance_config: SurveillanceConfig,
    /// How often to publish updated parameters (ms)
    pub publish_interval_ms: u64,
}

impl Default for TradingRiskConfig {
    fn default() -> Self {
        Self {
            default_instrument_limits: InstrumentLimits::default(),
            default_strategy_limits: StrategyLimits::default(),
            drawdown_limits: DrawdownLimits::default(),
            cost_limits: CostLimits::default(),
            surveillance_config: SurveillanceConfig::default(),
            publish_interval_ms: 100,
        }
    }
}

/// Active Trading Risk Manager
pub struct TradingRiskManager {
    config: TradingRiskConfig,
    /// Per-instrument limits (overrides defaults)
    instrument_limits: HashMap<String, InstrumentLimits>,
    /// Per-strategy limits (overrides defaults)
    strategy_limits: HashMap<String, StrategyLimits>,
    /// Market surveillance
    surveillance: BasicSurveillance,
    /// Current risk parameters (published to OM)
    current_params: Arc<RwLock<TradingRiskParameters>>,
    /// Parameter version counter
    version: AtomicU64,
    /// Tracking: current daily PnL
    daily_pnl: Decimal,
    /// Tracking: peak PnL (for drawdown)
    peak_pnl: Decimal,
    /// Trading halted?
    trading_halted: bool,
    /// Halt reason
    halt_reason: Option<String>,
}

impl TradingRiskManager {
    pub fn new(config: TradingRiskConfig) -> Self {
        let surveillance = BasicSurveillance::new(config.surveillance_config.clone());

        Self {
            config,
            instrument_limits: HashMap::new(),
            strategy_limits: HashMap::new(),
            surveillance,
            current_params: Arc::new(RwLock::new(TradingRiskParameters::default())),
            version: AtomicU64::new(0),
            daily_pnl: Decimal::ZERO,
            peak_pnl: Decimal::ZERO,
            trading_halted: false,
            halt_reason: None,
        }
    }

    /// Set limits for a specific instrument
    pub fn set_instrument_limits(&mut self, instrument_id: &str, limits: InstrumentLimits) {
        self.instrument_limits
            .insert(instrument_id.to_string(), limits);
    }

    /// Set limits for a specific strategy
    pub fn set_strategy_limits(&mut self, strategy_id: &str, limits: StrategyLimits) {
        self.strategy_limits.insert(strategy_id.to_string(), limits);
    }

    /// Update with new PnL
    pub fn update_pnl(&mut self, pnl_change: Decimal) {
        self.daily_pnl += pnl_change;

        // Track peak
        if self.daily_pnl > self.peak_pnl {
            self.peak_pnl = self.daily_pnl;
        }

        // Check limits
        self.check_drawdown_limits();
    }

    /// Check and enforce drawdown limits
    fn check_drawdown_limits(&mut self) {
        // Copy values to avoid borrow issues
        let max_daily_loss = self.config.drawdown_limits.max_daily_loss;
        let max_drawdown_pct = self.config.drawdown_limits.max_drawdown_pct;

        // Daily loss limit
        if self.daily_pnl < -max_daily_loss {
            self.halt_trading(format!(
                "Daily loss limit breached: {} < -{}",
                self.daily_pnl, max_daily_loss
            ));
        }

        // Drawdown limit
        if self.peak_pnl > Decimal::ZERO {
            let drawdown = (self.peak_pnl - self.daily_pnl) / self.peak_pnl;
            if drawdown > max_drawdown_pct {
                self.halt_trading(format!(
                    "Drawdown limit breached: {:.2}% > {:.2}%",
                    drawdown * dec!(100),
                    max_drawdown_pct * dec!(100)
                ));
            }
        }
    }

    /// Halt all trading
    pub fn halt_trading(&mut self, reason: String) {
        if !self.trading_halted {
            error!("[RISK] Trading halted: {}", reason);
            self.trading_halted = true;
            self.halt_reason = Some(reason);
        }
    }

    /// Resume trading (manual intervention)
    pub fn resume_trading(&mut self) {
        if self.trading_halted {
            info!("[RISK] Trading resumed");
            self.trading_halted = false;
            self.halt_reason = None;
        }
    }

    /// Reset daily PnL (call at start of day)
    pub fn reset_daily(&mut self) {
        info!("[RISK] Daily reset: PnL was {}", self.daily_pnl);
        self.daily_pnl = Decimal::ZERO;
        self.peak_pnl = Decimal::ZERO;

        // Resume trading if halted due to daily limits
        if self.trading_halted
            && let Some(reason) = &self.halt_reason
            && reason.contains("Daily")
        {
            self.resume_trading();
        }
    }

    /// Update market surveillance with book data
    pub fn update_book(&mut self, instrument_id: &str, spread_bps: Decimal, tob_size: Decimal) {
        self.surveillance
            .update_book(instrument_id, spread_bps, tob_size);
    }

    /// Compute and publish updated risk parameters
    pub async fn publish_parameters(&self) -> TradingRiskParameters {
        let version = self.version.fetch_add(1, Ordering::SeqCst);

        // Build instrument limits
        let mut instrument_limits = HashMap::new();
        for (id, limits) in &self.instrument_limits {
            instrument_limits.insert(id.clone(), limits.clone());
        }

        // Build strategy limits
        let mut strategy_limits = HashMap::new();
        for (id, limits) in &self.strategy_limits {
            strategy_limits.insert(id.clone(), limits.clone());
        }

        // Build market quality from surveillance
        let mut market_quality = HashMap::new();
        for instrument_id in self.instrument_limits.keys() {
            let quality = self.surveillance.market_quality(instrument_id);
            let _alerts = self.surveillance.active_alerts(instrument_id);

            market_quality.insert(
                instrument_id.clone(),
                MarketQualityLimit {
                    tradeable: quality.score >= dec!(0.5) && !quality.manipulation_suspected,
                    reason: if quality.manipulation_suspected {
                        Some("Manipulation suspected".to_string())
                    } else if quality.score < dec!(0.5) {
                        Some("Poor market quality".to_string())
                    } else {
                        None
                    },
                    quality_score: quality.score,
                    spread_acceptable: matches!(
                        quality.spread_quality,
                        crate::surveillance::SpreadQuality::Normal
                            | crate::surveillance::SpreadQuality::Tight
                    ),
                    depth_acceptable: matches!(
                        quality.depth_quality,
                        crate::surveillance::DepthQuality::Normal
                            | crate::surveillance::DepthQuality::Deep
                    ),
                    manipulation_detected: quality.manipulation_suspected,
                },
            );
        }

        // Calculate current drawdown
        let current_drawdown_pct = if self.peak_pnl > Decimal::ZERO {
            (self.peak_pnl - self.daily_pnl) / self.peak_pnl
        } else {
            Decimal::ZERO
        };

        let params = TradingRiskParameters {
            trading_enabled: !self.trading_halted,
            disabled_reason: self.halt_reason.clone(),
            instrument_limits,
            strategy_limits,
            drawdown: DrawdownLimits {
                current_daily_pnl: self.daily_pnl,
                current_drawdown_pct,
                daily_limit_breached: self.daily_pnl < -self.config.drawdown_limits.max_daily_loss,
                drawdown_limit_breached: current_drawdown_pct
                    > self.config.drawdown_limits.max_drawdown_pct,
                ..self.config.drawdown_limits.clone()
            },
            cost: self.config.cost_limits.clone(),
            market_quality,
            timestamp: Utc::now(),
            version,
        };

        // Update the shared state
        let mut current = self.current_params.write().await;
        *current = params.clone();

        params
    }

    /// Get handle to current parameters (for async access)
    pub fn parameters_handle(&self) -> Arc<RwLock<TradingRiskParameters>> {
        self.current_params.clone()
    }

    /// Get current daily PnL
    pub fn daily_pnl(&self) -> Decimal {
        self.daily_pnl
    }

    /// Is trading halted?
    pub fn is_halted(&self) -> bool {
        self.trading_halted
    }

    /// Get surveillance
    pub fn surveillance(&self) -> &dyn MarketSurveillance {
        &self.surveillance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_risk_manager_creation() {
        let config = TradingRiskConfig::default();
        let manager = TradingRiskManager::new(config);

        assert!(!manager.is_halted());
        assert_eq!(manager.daily_pnl(), Decimal::ZERO);
    }

    #[tokio::test]
    async fn test_pnl_tracking() {
        let config = TradingRiskConfig::default();
        let mut manager = TradingRiskManager::new(config);

        manager.update_pnl(dec!(1000));
        assert_eq!(manager.daily_pnl(), dec!(1000));

        manager.update_pnl(dec!(-500));
        assert_eq!(manager.daily_pnl(), dec!(500));
    }

    #[tokio::test]
    async fn test_daily_loss_halt() {
        let config = TradingRiskConfig {
            drawdown_limits: DrawdownLimits {
                max_daily_loss: dec!(1000),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut manager = TradingRiskManager::new(config);

        manager.update_pnl(dec!(-1500));
        assert!(manager.is_halted());
    }

    #[tokio::test]
    async fn test_drawdown_halt() {
        let config = TradingRiskConfig {
            drawdown_limits: DrawdownLimits {
                max_drawdown_pct: dec!(0.10), // 10%
                ..Default::default()
            },
            ..Default::default()
        };
        let mut manager = TradingRiskManager::new(config);

        // Make some money first
        manager.update_pnl(dec!(10000));

        // Then lose 15%
        manager.update_pnl(dec!(-1500));

        assert!(manager.is_halted());
    }

    #[tokio::test]
    async fn test_resume_trading() {
        let config = TradingRiskConfig::default();
        let mut manager = TradingRiskManager::new(config);

        manager.halt_trading("Test halt".to_string());
        assert!(manager.is_halted());

        manager.resume_trading();
        assert!(!manager.is_halted());
    }

    #[tokio::test]
    async fn test_publish_parameters() {
        let config = TradingRiskConfig::default();
        let mut manager = TradingRiskManager::new(config);

        manager.set_instrument_limits(
            "BTC-USD",
            InstrumentLimits {
                max_position: dec!(10),
                ..Default::default()
            },
        );

        let params = manager.publish_parameters().await;

        assert!(params.trading_enabled);
        assert_eq!(params.position_limit("BTC-USD"), dec!(10));
    }

    #[tokio::test]
    async fn test_daily_reset() {
        let config = TradingRiskConfig {
            drawdown_limits: DrawdownLimits {
                max_daily_loss: dec!(1000),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut manager = TradingRiskManager::new(config);

        // Hit daily limit
        manager.update_pnl(dec!(-1500));
        assert!(manager.is_halted());

        // Reset should resume
        manager.reset_daily();
        assert!(!manager.is_halted());
        assert_eq!(manager.daily_pnl(), Decimal::ZERO);
    }
}
