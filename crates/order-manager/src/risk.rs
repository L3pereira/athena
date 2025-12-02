//! Risk Validation
//!
//! Validates portfolio targets against risk parameters provided by the
//! Trading Risk Manager. This module doesn't track state - it just validates.
//!
//! The TradingRiskManager (separate crate) actively monitors and publishes
//! TradingRiskParameters. This module consumes those parameters to validate
//! execution decisions.

use crate::aggregator::PortfolioTarget;
use crate::error::{Error, Result};
use crate::execution::ExecutionCostEstimate;
use crate::position::PositionTracker;
use athena_risk_manager::TradingRiskParameters;
use log::{info, warn};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Result of risk validation
#[derive(Debug, Clone)]
pub struct RiskResult {
    /// Did it pass?
    pub passed: bool,
    /// Adjusted target (may be reduced)
    pub adjusted_target: Option<PortfolioTarget>,
    /// List of violations (even warnings)
    pub violations: Vec<RiskViolation>,
}

impl RiskResult {
    pub fn pass(target: PortfolioTarget) -> Self {
        Self {
            passed: true,
            adjusted_target: Some(target),
            violations: Vec::new(),
        }
    }

    pub fn pass_with_adjustment(target: PortfolioTarget, violations: Vec<RiskViolation>) -> Self {
        Self {
            passed: true,
            adjusted_target: Some(target),
            violations,
        }
    }

    pub fn reject(violations: Vec<RiskViolation>) -> Self {
        Self {
            passed: false,
            adjusted_target: None,
            violations,
        }
    }
}

/// A risk violation
#[derive(Debug, Clone)]
pub struct RiskViolation {
    pub check: RiskCheckType,
    pub severity: Severity,
    pub message: String,
    pub requested_value: String,
    pub limit_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskCheckType {
    TradingDisabled,
    MarketQuality,
    PositionLimit,
    ExposureLimit,
    ConcentrationLimit,
    OrderSizeLimit,
    DailyLossLimit,
    DrawdownLimit,
    CostTooHigh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Informational, passed
    Info,
    /// Warning, passed but logged
    Warning,
    /// Adjusted target to fit within limits
    Adjusted,
    /// Hard rejection
    Rejected,
}

/// Validates portfolio targets against risk parameters
///
/// This is stateless - all state (drawdown, PnL) is tracked by TradingRiskManager
/// and reflected in the TradingRiskParameters it publishes.
pub struct RiskValidator;

impl RiskValidator {
    /// Validate a portfolio target against risk parameters
    pub fn validate(
        target: &PortfolioTarget,
        params: &TradingRiskParameters,
        positions: &PositionTracker,
        prices: &HashMap<String, Decimal>,
        cost_estimate: Option<&ExecutionCostEstimate>,
    ) -> RiskResult {
        let mut violations = Vec::new();
        let mut adjusted_target = target.clone();
        let mut hard_reject = false;

        // 0. Check if trading is enabled
        if !params.trading_enabled {
            violations.push(RiskViolation {
                check: RiskCheckType::TradingDisabled,
                severity: Severity::Rejected,
                message: params
                    .disabled_reason
                    .clone()
                    .unwrap_or_else(|| "Trading disabled".to_string()),
                requested_value: "trade".to_string(),
                limit_value: "disabled".to_string(),
            });
            return RiskResult::reject(violations);
        }

        // 1. Check market quality
        if !params.can_trade(&target.instrument_id) {
            let reason = params
                .market_quality
                .get(&target.instrument_id)
                .and_then(|mq| mq.reason.clone())
                .unwrap_or_else(|| "Poor market quality".to_string());

            violations.push(RiskViolation {
                check: RiskCheckType::MarketQuality,
                severity: Severity::Rejected,
                message: reason,
                requested_value: target.instrument_id.clone(),
                limit_value: "tradeable".to_string(),
            });
            hard_reject = true;
        }

        // 2. Check drawdown limits
        if params.drawdown.should_halt() {
            violations.push(RiskViolation {
                check: RiskCheckType::DrawdownLimit,
                severity: Severity::Rejected,
                message: if params.drawdown.daily_limit_breached {
                    format!(
                        "Daily loss {} exceeds limit",
                        params.drawdown.current_daily_pnl
                    )
                } else {
                    format!(
                        "Drawdown {:.2}% exceeds limit",
                        params.drawdown.current_drawdown_pct * dec!(100)
                    )
                },
                requested_value: "trade".to_string(),
                limit_value: "halted".to_string(),
            });
            hard_reject = true;
        }

        // 3. Check position limit
        let max_pos = params.position_limit(&target.instrument_id);
        if target.target_position.abs() > max_pos {
            violations.push(RiskViolation {
                check: RiskCheckType::PositionLimit,
                severity: Severity::Adjusted,
                message: format!(
                    "Position {} exceeds limit {} for {}",
                    target.target_position, max_pos, target.instrument_id
                ),
                requested_value: target.target_position.to_string(),
                limit_value: max_pos.to_string(),
            });

            adjusted_target.target_position = if target.target_position > Decimal::ZERO {
                max_pos
            } else {
                -max_pos
            };
        }

        // 4. Check total exposure
        let price = prices
            .get(&target.instrument_id)
            .copied()
            .unwrap_or(Decimal::ONE);
        let target_notional = adjusted_target.target_position.abs() * price;

        let mut other_notional = Decimal::ZERO;
        for (instrument_id, pos) in positions.all_portfolio_positions() {
            if instrument_id != &target.instrument_id {
                let p = prices.get(instrument_id).copied().unwrap_or(Decimal::ONE);
                other_notional += pos.quantity.abs() * p;
            }
        }

        // Get instrument limit for notional
        let max_notional = params
            .instrument_limits
            .get(&target.instrument_id)
            .map(|l| l.max_notional)
            .unwrap_or(dec!(1_000_000));

        if target_notional > max_notional {
            let max_allowed_position = max_notional / price;
            adjusted_target.target_position = if adjusted_target.target_position > Decimal::ZERO {
                adjusted_target.target_position.min(max_allowed_position)
            } else {
                adjusted_target.target_position.max(-max_allowed_position)
            };

            violations.push(RiskViolation {
                check: RiskCheckType::ExposureLimit,
                severity: Severity::Adjusted,
                message: format!(
                    "Notional {} exceeds limit {}",
                    target_notional, max_notional
                ),
                requested_value: target_notional.to_string(),
                limit_value: max_notional.to_string(),
            });
        }

        // 5. Apply size multiplier from drawdown
        let size_mult = params.drawdown.size_multiplier();
        if size_mult < Decimal::ONE && size_mult > Decimal::ZERO {
            let original = adjusted_target.target_position;
            adjusted_target.target_position *= size_mult;

            violations.push(RiskViolation {
                check: RiskCheckType::DrawdownLimit,
                severity: Severity::Adjusted,
                message: format!(
                    "Size reduced by {:.0}% due to approaching limits",
                    (Decimal::ONE - size_mult) * dec!(100)
                ),
                requested_value: original.to_string(),
                limit_value: adjusted_target.target_position.to_string(),
            });
        }

        // 6. Check execution cost vs alpha
        if let Some(cost) = cost_estimate {
            if let Some(alpha) = target.combined_alpha {
                let alpha_bps = alpha * dec!(10000); // Convert to bps
                if !params.is_cost_acceptable(cost.total_cost_bps, alpha_bps) {
                    violations.push(RiskViolation {
                        check: RiskCheckType::CostTooHigh,
                        severity: Severity::Rejected,
                        message: format!(
                            "Execution cost {:.2} bps exceeds acceptable ratio for alpha {:.2} bps",
                            cost.total_cost_bps, alpha_bps
                        ),
                        requested_value: cost.total_cost_bps.to_string(),
                        limit_value: format!(
                            "{:.2}% of alpha",
                            params.cost.max_cost_to_alpha_ratio * dec!(100)
                        ),
                    });
                    hard_reject = true;
                }
            }

            // Check minimum alpha requirement
            if let Some(alpha) = target.combined_alpha {
                let alpha_bps = alpha * dec!(10000);
                if alpha_bps.abs() < params.cost.min_alpha_bps {
                    violations.push(RiskViolation {
                        check: RiskCheckType::CostTooHigh,
                        severity: Severity::Warning,
                        message: format!(
                            "Alpha {:.2} bps below minimum threshold {:.2} bps",
                            alpha_bps, params.cost.min_alpha_bps
                        ),
                        requested_value: alpha_bps.to_string(),
                        limit_value: params.cost.min_alpha_bps.to_string(),
                    });
                }
            }
        }

        // Return result
        if hard_reject {
            for v in &violations {
                warn!("[RISK REJECTED] {:?}: {}", v.check, v.message);
            }
            RiskResult::reject(violations)
        } else if violations.iter().any(|v| v.severity == Severity::Adjusted) {
            for v in &violations {
                info!("[RISK ADJUSTED] {:?}: {}", v.check, v.message);
            }
            RiskResult::pass_with_adjustment(adjusted_target, violations)
        } else {
            if !violations.is_empty() {
                for v in &violations {
                    info!("[RISK WARNING] {:?}: {}", v.check, v.message);
                }
            }
            RiskResult::pass_with_adjustment(adjusted_target, violations)
        }
    }

    /// Quick check if an order size is allowed
    pub fn check_order_size(
        quantity: Decimal,
        instrument_id: &str,
        params: &TradingRiskParameters,
    ) -> Result<()> {
        let max_size = params.max_order_size(instrument_id);
        if quantity.abs() > max_size {
            Err(Error::RiskCheckFailed {
                reason: format!("Order size {} exceeds limit {}", quantity, max_size),
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::Urgency;
    use athena_risk_manager::{DrawdownLimits, InstrumentLimits, MarketQualityLimit};
    use chrono::Utc;

    fn make_target(instrument: &str, position: Decimal) -> PortfolioTarget {
        PortfolioTarget {
            instrument_id: instrument.to_string(),
            target_position: position,
            combined_alpha: Some(dec!(0.001)), // 10 bps
            combined_confidence: Decimal::ONE,
            urgency: Urgency::Normal,
            stop_loss: None,
            take_profit: None,
            contributing_signals: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    fn default_params() -> TradingRiskParameters {
        let mut params = TradingRiskParameters::default();
        params.instrument_limits.insert(
            "BTC-USD".to_string(),
            InstrumentLimits {
                max_position: dec!(10),
                max_order_size: dec!(5),
                ..Default::default()
            },
        );
        params
    }

    #[test]
    fn test_passes_within_limits() {
        let params = default_params();
        let positions = PositionTracker::new();
        let prices = HashMap::new();

        let target = make_target("BTC-USD", dec!(5));
        let result = RiskValidator::validate(&target, &params, &positions, &prices, None);

        assert!(result.passed);
        assert_eq!(result.adjusted_target.unwrap().target_position, dec!(5));
    }

    #[test]
    fn test_adjusts_position_limit() {
        let params = default_params();
        let positions = PositionTracker::new();
        let prices = HashMap::new();

        let target = make_target("BTC-USD", dec!(15)); // Over 10 limit
        let result = RiskValidator::validate(&target, &params, &positions, &prices, None);

        assert!(result.passed);
        assert_eq!(result.adjusted_target.unwrap().target_position, dec!(10));
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.check == RiskCheckType::PositionLimit)
        );
    }

    #[test]
    fn test_rejects_when_trading_disabled() {
        let mut params = default_params();
        params.trading_enabled = false;
        params.disabled_reason = Some("Emergency halt".to_string());

        let positions = PositionTracker::new();
        let prices = HashMap::new();
        let target = make_target("BTC-USD", dec!(1));

        let result = RiskValidator::validate(&target, &params, &positions, &prices, None);
        assert!(!result.passed);
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.check == RiskCheckType::TradingDisabled)
        );
    }

    #[test]
    fn test_rejects_poor_market_quality() {
        let mut params = default_params();
        params.market_quality.insert(
            "BTC-USD".to_string(),
            MarketQualityLimit {
                tradeable: false,
                reason: Some("Manipulation detected".to_string()),
                ..Default::default()
            },
        );

        let positions = PositionTracker::new();
        let prices = HashMap::new();
        let target = make_target("BTC-USD", dec!(1));

        let result = RiskValidator::validate(&target, &params, &positions, &prices, None);
        assert!(!result.passed);
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.check == RiskCheckType::MarketQuality)
        );
    }

    #[test]
    fn test_rejects_on_drawdown() {
        let mut params = default_params();
        params.drawdown = DrawdownLimits {
            daily_limit_breached: true,
            ..Default::default()
        };

        let positions = PositionTracker::new();
        let prices = HashMap::new();
        let target = make_target("BTC-USD", dec!(1));

        let result = RiskValidator::validate(&target, &params, &positions, &prices, None);
        assert!(!result.passed);
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.check == RiskCheckType::DrawdownLimit)
        );
    }

    #[test]
    fn test_reduces_size_approaching_limit() {
        let mut params = default_params();
        // Set size multiplier to 0.5 by having drawdown in warning zone
        params.drawdown = DrawdownLimits {
            max_daily_loss: dec!(50_000),
            daily_loss_warning: dec!(25_000),
            current_daily_pnl: dec!(-37_500), // Halfway between warning and limit
            daily_limit_breached: false,
            ..Default::default()
        };

        let positions = PositionTracker::new();
        let prices = HashMap::new();
        let target = make_target("BTC-USD", dec!(10));

        let result = RiskValidator::validate(&target, &params, &positions, &prices, None);
        assert!(result.passed);

        let adjusted = result.adjusted_target.unwrap();
        assert!(adjusted.target_position < dec!(10)); // Size reduced
    }

    #[test]
    fn test_cost_validation() {
        let params = default_params();
        let positions = PositionTracker::new();
        let prices = HashMap::new();

        // Target with 10 bps alpha
        let target = make_target("BTC-USD", dec!(1));

        // Cost of 8 bps (80% of alpha) should fail (default max is 50%)
        let high_cost = ExecutionCostEstimate {
            spread_cost_bps: dec!(5),
            market_impact_bps: dec!(3),
            fee_bps: dec!(0),
            total_cost_bps: dec!(8),
        };

        let result =
            RiskValidator::validate(&target, &params, &positions, &prices, Some(&high_cost));
        assert!(!result.passed);
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.check == RiskCheckType::CostTooHigh)
        );
    }

    #[test]
    fn test_order_size_check() {
        let params = default_params();

        assert!(RiskValidator::check_order_size(dec!(3), "BTC-USD", &params).is_ok());
        assert!(RiskValidator::check_order_size(dec!(5), "BTC-USD", &params).is_ok());
        assert!(RiskValidator::check_order_size(dec!(6), "BTC-USD", &params).is_err());
    }
}
