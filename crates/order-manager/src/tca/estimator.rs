//! Pre-Trade Cost Estimation
//!
//! Estimates execution costs before trading using market impact models.
//!
//! # Cost Components
//!
//! ```text
//! Total Cost = Spread Cost + Market Impact + Timing Risk + Fees
//!            = (spread/2) + f(Q,V,σ) + λσ²T + fees
//! ```
//!
//! Where:
//! - Spread Cost: Half the bid-ask spread for crossing
//! - Market Impact: Model-based impact estimate
//! - Timing Risk: Volatility exposure during execution
//! - Fees: Exchange and broker fees

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use super::models::{AlmgrenChrissModel, ImpactModel, SquareRootModel};
use super::{MarketState, OrderSpec};

/// Configuration for TCA estimator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcaEstimatorConfig {
    /// Trading fees in basis points
    pub fee_bps: Decimal,
    /// Whether to include timing risk
    pub include_timing_risk: bool,
    /// Confidence level for timing risk (e.g., 0.95 for 95% VaR)
    pub timing_risk_confidence: Decimal,
    /// Which impact model to use
    pub impact_model: ImpactModelType,
    /// Minimum spread assumption if not available
    pub min_spread_bps: Decimal,
}

impl Default for TcaEstimatorConfig {
    fn default() -> Self {
        Self {
            fee_bps: dec!(5),
            include_timing_risk: true,
            timing_risk_confidence: dec!(0.95),
            impact_model: ImpactModelType::SquareRoot,
            min_spread_bps: dec!(5),
        }
    }
}

/// Type of impact model to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactModelType {
    /// Kyle linear model
    Kyle,
    /// Almgren-Chriss temporary + permanent
    AlmgrenChriss,
    /// Square root (Bouchaud)
    SquareRoot,
}

/// Pre-trade cost estimate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcaEstimate {
    /// Instrument being traded
    pub instrument_id: String,
    /// Order quantity
    pub quantity: Decimal,
    /// Is buy order
    pub is_buy: bool,

    // Cost breakdown (all in basis points)
    /// Half spread cost for crossing
    pub spread_cost_bps: Decimal,
    /// Market impact cost
    pub impact_cost_bps: Decimal,
    /// Timing risk (volatility during execution)
    pub timing_risk_bps: Decimal,
    /// Trading fees
    pub fee_bps: Decimal,
    /// Total expected cost
    pub total_cost_bps: Decimal,

    // Additional metrics
    /// Participation rate (order size / ADV)
    pub participation_rate: Decimal,
    /// Temporary impact component
    pub temporary_impact_bps: Decimal,
    /// Permanent impact component
    pub permanent_impact_bps: Decimal,
    /// 95% confidence upper bound on cost
    pub cost_upper_bound_bps: Decimal,
    /// 95% confidence lower bound on cost
    pub cost_lower_bound_bps: Decimal,

    // Context
    /// Model used for estimation
    pub model_used: String,
}

impl TcaEstimate {
    /// Convert to absolute cost given notional value
    pub fn absolute_cost(&self, notional: Decimal) -> Decimal {
        self.total_cost_bps * notional / dec!(10000)
    }

    /// Check if expected cost exceeds a threshold
    pub fn exceeds_threshold(&self, threshold_bps: Decimal) -> bool {
        self.total_cost_bps > threshold_bps
    }

    /// Get cost breakdown as a formatted string
    pub fn cost_breakdown(&self) -> String {
        format!(
            "Spread: {:.1} bps | Impact: {:.1} bps | Timing: {:.1} bps | Fees: {:.1} bps | Total: {:.1} bps",
            self.spread_cost_bps,
            self.impact_cost_bps,
            self.timing_risk_bps,
            self.fee_bps,
            self.total_cost_bps
        )
    }
}

/// Pre-trade cost estimator
///
/// Estimates expected execution costs using market impact models
/// and current market conditions.
pub struct TcaEstimator {
    config: TcaEstimatorConfig,
    impact_model: Box<dyn ImpactModel>,
}

impl TcaEstimator {
    /// Create new estimator with configuration
    pub fn new(config: TcaEstimatorConfig) -> Self {
        let impact_model: Box<dyn ImpactModel> = match config.impact_model {
            ImpactModelType::Kyle => Box::new(super::models::KyleModel::with_lambda(dec!(10))),
            ImpactModelType::AlmgrenChriss => Box::new(AlmgrenChrissModel::default_model()),
            ImpactModelType::SquareRoot => Box::new(SquareRootModel::default_model()),
        };

        Self {
            config,
            impact_model,
        }
    }

    /// Create with custom impact model
    pub fn with_model(config: TcaEstimatorConfig, model: Box<dyn ImpactModel>) -> Self {
        Self {
            config,
            impact_model: model,
        }
    }

    /// Estimate execution cost for an order
    pub fn estimate(&self, order: &OrderSpec, market: &MarketState) -> TcaEstimate {
        let quantity = order.quantity;
        let is_buy = order.is_buy;

        // 1. Spread cost (half spread for crossing)
        let spread_bps = market
            .current_spread_bps()
            .unwrap_or(self.config.min_spread_bps);
        let spread_cost_bps = spread_bps / Decimal::TWO;

        // 2. Market impact
        let impact = self.impact_model.calculate_impact(quantity, market);
        let impact_cost_bps = impact.total_bps;
        let temporary_impact_bps = impact.temporary_bps;
        let permanent_impact_bps = impact.permanent_bps;

        // 3. Timing risk (volatility during execution)
        let timing_risk_bps = if self.config.include_timing_risk {
            self.calculate_timing_risk(order, market)
        } else {
            Decimal::ZERO
        };

        // 4. Fees
        let fee_bps = self.config.fee_bps;

        // Total expected cost
        let total_cost_bps = spread_cost_bps + impact_cost_bps + timing_risk_bps + fee_bps;

        // Confidence interval (simplified normal approximation)
        // Variance primarily comes from timing risk
        let volatility_contribution = timing_risk_bps * dec!(1.5); // Simplified
        let cost_upper_bound_bps = total_cost_bps + volatility_contribution;
        let cost_lower_bound_bps = (total_cost_bps - volatility_contribution).max(Decimal::ZERO);

        // Participation rate
        let participation_rate = market.participation_rate(quantity);

        TcaEstimate {
            instrument_id: order.instrument_id.clone(),
            quantity,
            is_buy,
            spread_cost_bps,
            impact_cost_bps,
            timing_risk_bps,
            fee_bps,
            total_cost_bps,
            participation_rate,
            temporary_impact_bps,
            permanent_impact_bps,
            cost_upper_bound_bps,
            cost_lower_bound_bps,
            model_used: self.impact_model.name().to_string(),
        }
    }

    /// Calculate timing risk (volatility exposure during execution)
    ///
    /// Timing risk = σ × √T × Z_α × price
    /// Where Z_α is the quantile for desired confidence
    fn calculate_timing_risk(&self, order: &OrderSpec, market: &MarketState) -> Decimal {
        // Convert time horizon to fraction of year (assuming 252 trading days)
        let time_fraction =
            Decimal::from(order.time_horizon_secs) / Decimal::from(252 * 6 * 60 * 60); // Trading seconds per year

        // Z-score for confidence level (simplified)
        let z_score = match self.config.timing_risk_confidence.to_string().as_str() {
            "0.99" => dec!(2.326),
            "0.95" => dec!(1.645),
            "0.90" => dec!(1.282),
            _ => dec!(1.645), // Default to 95%
        };

        // Timing risk in price terms, converted to bps
        // Risk increases with √time and volatility
        let sqrt_time = super::models::sqrt_decimal(time_fraction);

        market.volatility * sqrt_time * z_score * dec!(10000) / Decimal::TWO
    }

    /// Estimate cost for multiple scenarios (sensitivity analysis)
    pub fn sensitivity_analysis(
        &self,
        order: &OrderSpec,
        market: &MarketState,
        quantity_multipliers: &[Decimal],
    ) -> Vec<(Decimal, TcaEstimate)> {
        quantity_multipliers
            .iter()
            .map(|mult| {
                let mut adjusted_order = order.clone();
                adjusted_order.quantity = order.quantity * *mult;
                let estimate = self.estimate(&adjusted_order, market);
                (*mult, estimate)
            })
            .collect()
    }

    /// Compare estimates across different impact models
    pub fn compare_models(
        order: &OrderSpec,
        market: &MarketState,
        config: &TcaEstimatorConfig,
    ) -> Vec<(String, TcaEstimate)> {
        let models = vec![
            ImpactModelType::Kyle,
            ImpactModelType::AlmgrenChriss,
            ImpactModelType::SquareRoot,
        ];

        models
            .into_iter()
            .map(|model_type| {
                let mut model_config = config.clone();
                model_config.impact_model = model_type;
                let estimator = TcaEstimator::new(model_config);
                let estimate = estimator.estimate(order, market);
                (estimate.model_used.clone(), estimate)
            })
            .collect()
    }

    /// Check if order should be executed given alpha expectation
    ///
    /// Returns true if expected alpha exceeds expected cost
    pub fn is_alpha_sufficient(
        &self,
        order: &OrderSpec,
        market: &MarketState,
        expected_alpha_bps: Decimal,
        cost_buffer_multiplier: Decimal,
    ) -> (bool, TcaEstimate) {
        let estimate = self.estimate(order, market);
        let required_alpha = estimate.total_cost_bps * cost_buffer_multiplier;
        (expected_alpha_bps > required_alpha, estimate)
    }
}

impl Default for TcaEstimator {
    fn default() -> Self {
        Self::new(TcaEstimatorConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_market() -> MarketState {
        MarketState::new("BTC-USD")
            .with_bbo(dec!(50000), dec!(50010))
            .with_adv(dec!(10000))
            .with_volatility(dec!(0.50))
            .with_spread(dec!(2))
    }

    fn make_order() -> OrderSpec {
        OrderSpec::new("BTC-USD", true, dec!(100), 3600)
    }

    #[test]
    fn test_basic_estimate() {
        let estimator = TcaEstimator::default();
        let market = make_market();
        let order = make_order();

        let estimate = estimator.estimate(&order, &market);

        assert!(estimate.total_cost_bps > Decimal::ZERO);
        assert!(estimate.spread_cost_bps > Decimal::ZERO);
        assert!(estimate.impact_cost_bps > Decimal::ZERO);
        assert_eq!(estimate.participation_rate, dec!(0.01)); // 1%

        println!("{}", estimate.cost_breakdown());
    }

    #[test]
    fn test_cost_scales_with_size() {
        let estimator = TcaEstimator::default();
        let market = make_market();

        let small_order = OrderSpec::new("BTC-USD", true, dec!(10), 3600);
        let large_order = OrderSpec::new("BTC-USD", true, dec!(1000), 3600);

        let small_estimate = estimator.estimate(&small_order, &market);
        let large_estimate = estimator.estimate(&large_order, &market);

        // Large order should have higher impact cost
        assert!(large_estimate.impact_cost_bps > small_estimate.impact_cost_bps);

        println!("Small: {}", small_estimate.cost_breakdown());
        println!("Large: {}", large_estimate.cost_breakdown());
    }

    #[test]
    fn test_alpha_check() {
        let estimator = TcaEstimator::default();
        let market = make_market();
        let order = make_order();

        // First, get the estimate to see actual cost
        let estimate = estimator.estimate(&order, &market);
        println!("Estimated cost: {} bps", estimate.total_cost_bps);

        // High alpha (well above cost × buffer) should pass
        let high_alpha = estimate.total_cost_bps * dec!(3); // 3x the cost
        let (should_trade, _) =
            estimator.is_alpha_sufficient(&order, &market, high_alpha, dec!(1.5));
        assert!(should_trade);

        // Low alpha (below cost) should fail
        let (should_not_trade, _) =
            estimator.is_alpha_sufficient(&order, &market, dec!(1), dec!(1.5));
        assert!(!should_not_trade);
    }

    #[test]
    fn test_model_comparison() {
        let market = make_market();
        let order = make_order();
        let config = TcaEstimatorConfig::default();

        let comparisons = TcaEstimator::compare_models(&order, &market, &config);

        assert_eq!(comparisons.len(), 3);
        for (model_name, estimate) in comparisons {
            println!("{}: {}", model_name, estimate.cost_breakdown());
        }
    }

    #[test]
    fn test_sensitivity_analysis() {
        let estimator = TcaEstimator::default();
        let market = make_market();
        let order = make_order();

        let multipliers = vec![dec!(0.5), dec!(1.0), dec!(2.0), dec!(5.0)];
        let results = estimator.sensitivity_analysis(&order, &market, &multipliers);

        assert_eq!(results.len(), 4);

        // Costs should increase with size (but not linearly due to sqrt impact)
        let costs: Vec<_> = results.iter().map(|(_, e)| e.total_cost_bps).collect();
        for i in 1..costs.len() {
            assert!(costs[i] > costs[i - 1]);
        }
    }

    #[test]
    fn test_absolute_cost() {
        let estimator = TcaEstimator::default();
        let market = make_market();
        let order = make_order();

        let estimate = estimator.estimate(&order, &market);

        // For $1M notional
        let notional = dec!(1_000_000);
        let absolute = estimate.absolute_cost(notional);

        println!("Cost for ${} notional: ${:.2}", notional, absolute);
        assert!(absolute > Decimal::ZERO);
    }
}
