//! Execution Planner
//!
//! Converts portfolio targets into executable orders:
//! - Calculates delta from current position
//! - Slices large orders into child orders
//! - Applies urgency-based execution style
//! - Generates order requests for the gateway

use crate::aggregator::PortfolioTarget;
use crate::position::PositionTracker;
use crate::signal::Urgency;
use athena_gateway::messages::order::{OrderRequest, OrderSide, TimeInForceWire};
use chrono::{DateTime, Utc};
use log::info;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Execution configuration
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// Maximum order size (for slicing)
    pub max_order_size: Decimal,
    /// Minimum order size
    pub min_order_size: Decimal,
    /// Tick sizes by instrument
    pub tick_sizes: HashMap<String, Decimal>,
    /// Default tick size
    pub default_tick_size: Decimal,
    /// How aggressive to price passive orders (in ticks from best)
    pub passive_offset_ticks: Decimal,
    /// How aggressive to price normal orders
    pub normal_offset_ticks: Decimal,
    /// Aggressive orders cross spread by this many ticks
    pub aggressive_cross_ticks: Decimal,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_order_size: dec!(10),
            min_order_size: dec!(0.001),
            tick_sizes: HashMap::new(),
            default_tick_size: dec!(0.01),
            passive_offset_ticks: dec!(2),   // 2 ticks behind best
            normal_offset_ticks: dec!(0),    // At best bid/ask
            aggressive_cross_ticks: dec!(1), // 1 tick through spread
        }
    }
}

/// An execution order (what we send to gateway)
#[derive(Debug, Clone)]
pub struct ExecutionOrder {
    /// Client order ID
    pub client_order_id: String,
    /// Instrument
    pub instrument_id: String,
    /// Side
    pub side: OrderSide,
    /// Quantity
    pub quantity: Decimal,
    /// Price (None for market orders)
    pub price: Option<Decimal>,
    /// Time in force
    pub time_in_force: TimeInForceWire,
    /// Parent order ID (for sliced orders)
    pub parent_order_id: Option<String>,
    /// Strategy attribution
    pub strategy_attribution: Vec<(String, Decimal)>, // (strategy_id, weight)
}

impl ExecutionOrder {
    /// Convert to gateway OrderRequest
    pub fn to_request(&self) -> OrderRequest {
        match self.price {
            Some(price) => OrderRequest::limit(
                &self.client_order_id,
                &self.instrument_id,
                self.side,
                self.quantity,
                price,
                self.time_in_force,
            ),
            None => OrderRequest::market(
                &self.client_order_id,
                &self.instrument_id,
                self.side,
                self.quantity,
            ),
        }
    }
}

/// Execution plan for a target
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Target we're executing
    pub target: PortfolioTarget,
    /// Current position
    pub current_position: Decimal,
    /// Delta to trade
    pub delta: Decimal,
    /// Child orders
    pub orders: Vec<ExecutionOrder>,
    /// When plan was created
    pub timestamp: DateTime<Utc>,
}

impl ExecutionPlan {
    /// Total quantity across all orders
    pub fn total_quantity(&self) -> Decimal {
        self.orders.iter().map(|o| o.quantity).sum()
    }

    /// Number of orders
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }
}

/// Plans execution of portfolio targets
pub struct ExecutionPlanner {
    config: ExecutionConfig,
    /// Order ID counter
    order_counter: u64,
}

impl ExecutionPlanner {
    pub fn new(config: ExecutionConfig) -> Self {
        Self {
            config,
            order_counter: 0,
        }
    }

    /// Generate next order ID
    fn next_order_id(&mut self) -> String {
        self.order_counter += 1;
        format!("exec-{}", self.order_counter)
    }

    /// Create execution plan for a target
    pub fn plan_execution(
        &mut self,
        target: &PortfolioTarget,
        positions: &PositionTracker,
        market_data: &MarketSnapshot,
    ) -> Option<ExecutionPlan> {
        // Get current position
        let current_position = positions
            .portfolio_position(&target.instrument_id)
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);

        // Calculate delta
        let delta = target.target_position - current_position;

        // Check if delta is significant
        if delta.abs() < self.config.min_order_size {
            return None;
        }

        // Determine side
        let side = if delta > Decimal::ZERO {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let abs_delta = delta.abs();

        // Get tick size
        let tick = self
            .config
            .tick_sizes
            .get(&target.instrument_id)
            .copied()
            .unwrap_or(self.config.default_tick_size);

        // Calculate execution price based on urgency
        let price = self.calculate_price(&target.urgency, side, market_data, tick);

        // Determine TIF based on urgency
        let tif = match target.urgency {
            Urgency::Immediate => TimeInForceWire::Ioc,
            _ => TimeInForceWire::Gtc,
        };

        // Strategy attribution
        let attribution: Vec<(String, Decimal)> = target
            .contributing_signals
            .iter()
            .map(|c| (c.strategy_id.clone(), c.weight))
            .collect();

        // Slice into child orders if needed
        let orders = self.slice_order(
            &target.instrument_id,
            side,
            abs_delta,
            price,
            tif,
            &attribution,
        );

        info!(
            "[EXEC] {} {} {:.4} @ {:?} (delta from {:.4} to {:.4})",
            target.instrument_id,
            if delta > Decimal::ZERO { "BUY" } else { "SELL" },
            abs_delta,
            price,
            current_position,
            target.target_position
        );

        Some(ExecutionPlan {
            target: target.clone(),
            current_position,
            delta,
            orders,
            timestamp: Utc::now(),
        })
    }

    /// Calculate execution price based on urgency
    fn calculate_price(
        &self,
        urgency: &Urgency,
        side: OrderSide,
        market: &MarketSnapshot,
        tick: Decimal,
    ) -> Option<Decimal> {
        let (best_bid, best_ask) = (market.best_bid, market.best_ask);

        match urgency {
            Urgency::Immediate => {
                // Market order - no price
                None
            }
            Urgency::Aggressive => {
                // Cross the spread
                match side {
                    OrderSide::Buy => {
                        best_ask.map(|a| a + self.config.aggressive_cross_ticks * tick)
                    }
                    OrderSide::Sell => {
                        best_bid.map(|b| b - self.config.aggressive_cross_ticks * tick)
                    }
                }
            }
            Urgency::Normal => {
                // At best bid/ask
                match side {
                    OrderSide::Buy => best_bid,
                    OrderSide::Sell => best_ask,
                }
            }
            Urgency::Passive => {
                // Behind best bid/ask
                match side {
                    OrderSide::Buy => best_bid.map(|b| b - self.config.passive_offset_ticks * tick),
                    OrderSide::Sell => {
                        best_ask.map(|a| a + self.config.passive_offset_ticks * tick)
                    }
                }
            }
        }
    }

    /// Slice a large order into child orders
    fn slice_order(
        &mut self,
        instrument_id: &str,
        side: OrderSide,
        total_quantity: Decimal,
        price: Option<Decimal>,
        tif: TimeInForceWire,
        attribution: &[(String, Decimal)],
    ) -> Vec<ExecutionOrder> {
        let mut orders = Vec::new();
        let mut remaining = total_quantity;

        // Generate parent order ID for tracking
        let parent_id = if total_quantity > self.config.max_order_size {
            Some(self.next_order_id())
        } else {
            None
        };

        while remaining >= self.config.min_order_size {
            let qty = remaining.min(self.config.max_order_size);
            remaining -= qty;

            let order_id = self.next_order_id();

            orders.push(ExecutionOrder {
                client_order_id: order_id,
                instrument_id: instrument_id.to_string(),
                side,
                quantity: qty,
                price,
                time_in_force: tif,
                parent_order_id: parent_id.clone(),
                strategy_attribution: attribution.to_vec(),
            });
        }

        orders
    }

    /// Plan multiple targets at once
    pub fn plan_all(
        &mut self,
        targets: &[PortfolioTarget],
        positions: &PositionTracker,
        market_data: &HashMap<String, MarketSnapshot>,
    ) -> Vec<ExecutionPlan> {
        targets
            .iter()
            .filter_map(|target| {
                let snapshot = market_data.get(&target.instrument_id)?;
                self.plan_execution(target, positions, snapshot)
            })
            .collect()
    }
}

/// Market data snapshot for execution pricing
#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    pub instrument_id: String,
    pub best_bid: Option<Decimal>,
    pub best_ask: Option<Decimal>,
    pub bid_size: Option<Decimal>,
    pub ask_size: Option<Decimal>,
    pub last_price: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
}

impl MarketSnapshot {
    pub fn new(instrument_id: impl Into<String>) -> Self {
        Self {
            instrument_id: instrument_id.into(),
            best_bid: None,
            best_ask: None,
            bid_size: None,
            ask_size: None,
            last_price: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_bbo(mut self, bid: Decimal, ask: Decimal) -> Self {
        self.best_bid = Some(bid);
        self.best_ask = Some(ask);
        self
    }

    pub fn with_sizes(mut self, bid_size: Decimal, ask_size: Decimal) -> Self {
        self.bid_size = Some(bid_size);
        self.ask_size = Some(ask_size);
        self
    }

    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid, self.best_ask) {
            (Some(b), Some(a)) => Some((b + a) / dec!(2)),
            _ => None,
        }
    }

    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid, self.best_ask) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        }
    }

    /// Spread in basis points
    pub fn spread_bps(&self) -> Option<Decimal> {
        match (self.mid_price(), self.spread()) {
            (Some(mid), Some(spread)) if mid > Decimal::ZERO => Some(spread / mid * dec!(10000)),
            _ => None,
        }
    }
}

/// Estimated execution costs for a trade
///
/// This is what Risk Manager uses to validate cost vs alpha.
#[derive(Debug, Clone)]
pub struct ExecutionCostEstimate {
    /// Half spread cost (crossing bid-ask)
    pub spread_cost_bps: Decimal,
    /// Estimated market impact
    pub market_impact_bps: Decimal,
    /// Trading fees
    pub fee_bps: Decimal,
    /// Total estimated cost
    pub total_cost_bps: Decimal,
}

impl ExecutionCostEstimate {
    /// Create a new cost estimate
    pub fn new(spread_bps: Decimal, impact_bps: Decimal, fee_bps: Decimal) -> Self {
        Self {
            spread_cost_bps: spread_bps,
            market_impact_bps: impact_bps,
            fee_bps,
            total_cost_bps: spread_bps + impact_bps + fee_bps,
        }
    }

    /// Zero cost estimate
    pub fn zero() -> Self {
        Self {
            spread_cost_bps: Decimal::ZERO,
            market_impact_bps: Decimal::ZERO,
            fee_bps: Decimal::ZERO,
            total_cost_bps: Decimal::ZERO,
        }
    }
}

/// Configuration for cost estimation
#[derive(Debug, Clone)]
pub struct CostEstimatorConfig {
    /// Default fee in basis points
    pub default_fee_bps: Decimal,
    /// Market impact coefficient (impact = coeff * sqrt(qty / avg_volume))
    pub impact_coefficient: Decimal,
    /// Average daily volume by instrument (for impact estimation)
    pub avg_volumes: HashMap<String, Decimal>,
    /// Default average volume if not specified
    pub default_avg_volume: Decimal,
}

impl Default for CostEstimatorConfig {
    fn default() -> Self {
        Self {
            default_fee_bps: dec!(5),     // 5 bps fee
            impact_coefficient: dec!(10), // 10 bps * sqrt(participation)
            avg_volumes: HashMap::new(),
            default_avg_volume: dec!(1000),
        }
    }
}

/// Estimates execution costs for orders
///
/// Uses market snapshot and order size to estimate:
/// - Spread cost (half spread for crossing)
/// - Market impact (based on order size vs volume)
/// - Fees
pub struct CostEstimator {
    config: CostEstimatorConfig,
}

impl CostEstimator {
    pub fn new(config: CostEstimatorConfig) -> Self {
        Self { config }
    }

    /// Estimate execution cost for an order
    pub fn estimate(
        &self,
        instrument_id: &str,
        quantity: Decimal,
        _side: OrderSide,
        market: &MarketSnapshot,
    ) -> ExecutionCostEstimate {
        // Spread cost: half the spread for crossing
        let spread_bps = market.spread_bps().unwrap_or(dec!(10)) / dec!(2);

        // Market impact: coefficient * sqrt(quantity / avg_volume)
        let avg_volume = self
            .config
            .avg_volumes
            .get(instrument_id)
            .copied()
            .unwrap_or(self.config.default_avg_volume);

        let participation = if avg_volume > Decimal::ZERO {
            quantity / avg_volume
        } else {
            Decimal::ZERO
        };

        // Simple sqrt approximation for impact
        let impact_bps = self.config.impact_coefficient * self.approx_sqrt(participation);

        // Fees
        let fee_bps = self.config.default_fee_bps;

        ExecutionCostEstimate::new(spread_bps, impact_bps, fee_bps)
    }

    /// Estimate cost for an execution plan
    pub fn estimate_plan(
        &self,
        plan: &ExecutionPlan,
        market: &MarketSnapshot,
    ) -> ExecutionCostEstimate {
        if plan.orders.is_empty() {
            return ExecutionCostEstimate::zero();
        }

        let total_qty = plan.total_quantity();
        let side = plan.orders[0].side;
        self.estimate(&plan.target.instrument_id, total_qty, side, market)
    }

    /// Simple square root approximation using Newton's method
    fn approx_sqrt(&self, x: Decimal) -> Decimal {
        if x <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        // Newton's method: start with x/2, iterate
        let mut guess = x / dec!(2);
        if guess.is_zero() {
            guess = dec!(0.001);
        }

        for _ in 0..5 {
            guess = (guess + x / guess) / dec!(2);
        }
        guess
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregator::SignalContribution;
    use rust_decimal_macros::dec;

    fn make_target(instrument: &str, position: Decimal, urgency: Urgency) -> PortfolioTarget {
        PortfolioTarget {
            instrument_id: instrument.to_string(),
            target_position: position,
            combined_alpha: None,
            combined_confidence: Decimal::ONE,
            urgency,
            stop_loss: None,
            take_profit: None,
            contributing_signals: vec![SignalContribution {
                strategy_id: "test-strategy".to_string(),
                signal_position: position,
                weight: Decimal::ONE,
                weighted_contribution: position,
            }],
            timestamp: Utc::now(),
        }
    }

    fn make_market(instrument: &str, bid: Decimal, ask: Decimal) -> MarketSnapshot {
        MarketSnapshot::new(instrument).with_bbo(bid, ask)
    }

    #[test]
    fn test_basic_buy_plan() {
        let config = ExecutionConfig::default();
        let mut planner = ExecutionPlanner::new(config);
        let positions = PositionTracker::new();

        let target = make_target("BTC-USD", dec!(1), Urgency::Normal);
        let market = make_market("BTC-USD", dec!(50000), dec!(50010));

        let plan = planner
            .plan_execution(&target, &positions, &market)
            .unwrap();

        assert_eq!(plan.delta, dec!(1));
        assert_eq!(plan.orders.len(), 1);
        assert_eq!(plan.orders[0].side, OrderSide::Buy);
        assert_eq!(plan.orders[0].quantity, dec!(1));
        assert_eq!(plan.orders[0].price, Some(dec!(50000))); // At best bid
    }

    #[test]
    fn test_sell_plan_from_position() {
        let config = ExecutionConfig::default();
        let mut planner = ExecutionPlanner::new(config);

        // Setup existing position
        let mut positions = PositionTracker::new();
        positions.register_order("order-1", "test");
        positions.process_fill(&crate::position::Fill {
            order_id: "exch-1".to_string(),
            client_order_id: "order-1".to_string(),
            instrument_id: "BTC-USD".to_string(),
            side: crate::position::FillSide::Buy,
            quantity: dec!(2),
            price: dec!(50000),
            fee: Decimal::ZERO,
            fee_currency: "USD".to_string(),
            timestamp: Utc::now(),
        });

        // Target is 1, currently have 2 -> sell 1
        let target = make_target("BTC-USD", dec!(1), Urgency::Normal);
        let market = make_market("BTC-USD", dec!(50000), dec!(50010));

        let plan = planner
            .plan_execution(&target, &positions, &market)
            .unwrap();

        assert_eq!(plan.current_position, dec!(2));
        assert_eq!(plan.delta, dec!(-1)); // Need to sell 1
        assert_eq!(plan.orders[0].side, OrderSide::Sell);
    }

    #[test]
    fn test_order_slicing() {
        let config = ExecutionConfig {
            max_order_size: dec!(5),
            ..Default::default()
        };
        let mut planner = ExecutionPlanner::new(config);
        let positions = PositionTracker::new();

        // Large order should be sliced
        let target = make_target("BTC-USD", dec!(12), Urgency::Normal);
        let market = make_market("BTC-USD", dec!(50000), dec!(50010));

        let plan = planner
            .plan_execution(&target, &positions, &market)
            .unwrap();

        // 12 qty with max 5 -> 3 orders (5 + 5 + 2)
        assert_eq!(plan.orders.len(), 3);
        assert_eq!(plan.orders[0].quantity, dec!(5));
        assert_eq!(plan.orders[1].quantity, dec!(5));
        assert_eq!(plan.orders[2].quantity, dec!(2));

        // All should have same parent
        assert!(plan.orders[0].parent_order_id.is_some());
        assert_eq!(
            plan.orders[0].parent_order_id,
            plan.orders[1].parent_order_id
        );
    }

    #[test]
    fn test_urgency_pricing() {
        let config = ExecutionConfig {
            passive_offset_ticks: dec!(2),
            aggressive_cross_ticks: dec!(1),
            ..Default::default()
        };
        let mut planner = ExecutionPlanner::new(config.clone());
        let positions = PositionTracker::new();
        let market = make_market("BTC-USD", dec!(100.00), dec!(100.10));
        let tick = dec!(0.01);

        // Passive buy - behind best bid
        let passive = make_target("BTC-USD", dec!(1), Urgency::Passive);
        let plan = planner
            .plan_execution(&passive, &positions, &market)
            .unwrap();
        assert_eq!(plan.orders[0].price, Some(dec!(100.00) - dec!(2) * tick));

        // Normal buy - at best bid
        let normal = make_target("BTC-USD", dec!(1), Urgency::Normal);
        let plan = planner
            .plan_execution(&normal, &positions, &market)
            .unwrap();
        assert_eq!(plan.orders[0].price, Some(dec!(100.00)));

        // Aggressive buy - cross spread
        let aggressive = make_target("BTC-USD", dec!(1), Urgency::Aggressive);
        let plan = planner
            .plan_execution(&aggressive, &positions, &market)
            .unwrap();
        assert_eq!(plan.orders[0].price, Some(dec!(100.10) + tick));

        // Immediate - market order
        let immediate = make_target("BTC-USD", dec!(1), Urgency::Immediate);
        let plan = planner
            .plan_execution(&immediate, &positions, &market)
            .unwrap();
        assert!(plan.orders[0].price.is_none());
        assert_eq!(plan.orders[0].time_in_force, TimeInForceWire::Ioc);
    }

    #[test]
    fn test_no_plan_for_small_delta() {
        let config = ExecutionConfig {
            min_order_size: dec!(0.01),
            ..Default::default()
        };
        let mut planner = ExecutionPlanner::new(config);
        let positions = PositionTracker::new();

        let target = make_target("BTC-USD", dec!(0.001), Urgency::Normal);
        let market = make_market("BTC-USD", dec!(50000), dec!(50010));

        let plan = planner.plan_execution(&target, &positions, &market);
        assert!(plan.is_none());
    }

    #[test]
    fn test_attribution_preserved() {
        let config = ExecutionConfig::default();
        let mut planner = ExecutionPlanner::new(config);
        let positions = PositionTracker::new();

        let mut target = make_target("BTC-USD", dec!(1), Urgency::Normal);
        target.contributing_signals = vec![
            SignalContribution {
                strategy_id: "strategy-a".to_string(),
                signal_position: dec!(0.7),
                weight: dec!(0.7),
                weighted_contribution: dec!(0.7),
            },
            SignalContribution {
                strategy_id: "strategy-b".to_string(),
                signal_position: dec!(0.3),
                weight: dec!(0.3),
                weighted_contribution: dec!(0.3),
            },
        ];

        let market = make_market("BTC-USD", dec!(50000), dec!(50010));
        let plan = planner
            .plan_execution(&target, &positions, &market)
            .unwrap();

        // Attribution should be preserved
        assert_eq!(plan.orders[0].strategy_attribution.len(), 2);
        assert_eq!(plan.orders[0].strategy_attribution[0].0, "strategy-a");
        assert_eq!(plan.orders[0].strategy_attribution[0].1, dec!(0.7));
    }

    // Cost Estimator tests
    #[test]
    fn test_cost_estimate_spread() {
        let config = CostEstimatorConfig::default();
        let estimator = CostEstimator::new(config);

        // Market with 10 bps spread
        let market = MarketSnapshot::new("BTC-USD").with_bbo(dec!(50000), dec!(50050)); // 50/50005 = ~10 bps

        let estimate = estimator.estimate("BTC-USD", dec!(1), OrderSide::Buy, &market);

        // Spread cost should be half the spread
        assert!(estimate.spread_cost_bps > Decimal::ZERO);
        assert!(estimate.total_cost_bps > estimate.spread_cost_bps); // Should include fees
    }

    #[test]
    fn test_cost_estimate_impact() {
        let mut config = CostEstimatorConfig::default();
        config.avg_volumes.insert("BTC-USD".to_string(), dec!(100));
        let estimator = CostEstimator::new(config);

        let market = make_market("BTC-USD", dec!(50000), dec!(50010));

        // Small order - low impact
        let small = estimator.estimate("BTC-USD", dec!(1), OrderSide::Buy, &market);

        // Large order - higher impact
        let large = estimator.estimate("BTC-USD", dec!(50), OrderSide::Buy, &market);

        assert!(large.market_impact_bps > small.market_impact_bps);
    }

    #[test]
    fn test_cost_estimate_zero() {
        let zero = ExecutionCostEstimate::zero();
        assert_eq!(zero.total_cost_bps, Decimal::ZERO);
        assert_eq!(zero.spread_cost_bps, Decimal::ZERO);
    }

    #[test]
    fn test_spread_bps_calculation() {
        // 100 bid, 100.10 ask -> 0.10 spread, mid 100.05
        // spread_bps = 0.10 / 100.05 * 10000 ≈ 10 bps
        let market = make_market("BTC-USD", dec!(100), dec!(100.10));
        let spread_bps = market.spread_bps().unwrap();
        assert!(spread_bps > dec!(9) && spread_bps < dec!(11));
    }
}
