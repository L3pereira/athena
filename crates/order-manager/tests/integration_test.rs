//! Order Manager Integration Test
//!
//! Tests the full flow:
//! 1. Strategies emit signals
//! 2. Aggregator combines into portfolio targets
//! 3. Risk validator checks/adjusts against TradingRiskParameters
//! 4. Execution planner generates orders
//! 5. Position tracker processes fills and attributes PnL

use athena_order_manager::{
    aggregator::{AggregatorConfig, SignalAggregator, WeightingMethod},
    execution::{ExecutionConfig, ExecutionPlanner, MarketSnapshot},
    position::{Fill, FillSide, PositionTracker},
    risk::RiskValidator,
    signal::{Signal, Urgency},
};
use athena_risk_manager::{DrawdownLimits, InstrumentLimits, TradingRiskParameters};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Simulate the full order manager pipeline
#[test]
fn test_full_pipeline() {
    // === Setup Components ===
    let agg_config = AggregatorConfig {
        weighting: WeightingMethod::ConfidenceWeighted,
        ..Default::default()
    };
    let mut aggregator = SignalAggregator::new(agg_config);

    // Risk parameters (would come from TradingRiskManager in production)
    let mut risk_params = TradingRiskParameters::default();
    risk_params.instrument_limits.insert(
        "BTC-USD".to_string(),
        InstrumentLimits {
            max_position: dec!(10),
            max_order_size: dec!(5),
            ..Default::default()
        },
    );
    risk_params.instrument_limits.insert(
        "ETH-USD".to_string(),
        InstrumentLimits {
            max_position: dec!(10),
            max_order_size: dec!(5),
            ..Default::default()
        },
    );

    let exec_config = ExecutionConfig {
        max_order_size: dec!(5),
        min_order_size: dec!(0.01),
        ..Default::default()
    };
    let mut execution_planner = ExecutionPlanner::new(exec_config);

    let mut position_tracker = PositionTracker::new();

    // === Step 1: Strategies Emit Signals ===
    // Strategy A: Bullish on BTC, 90% confident, target long 5
    let signal_a = Signal::new("strategy-a", "BTC-USD", dec!(5))
        .with_alpha(dec!(0.02))
        .with_confidence(dec!(0.9))
        .with_urgency(Urgency::Normal);

    // Strategy B: Also bullish, 70% confident, target long 3
    let signal_b = Signal::new("strategy-b", "BTC-USD", dec!(3))
        .with_alpha(dec!(0.015))
        .with_confidence(dec!(0.7));

    // Strategy C: Bearish on ETH, 80% confident, target short 2
    let signal_c = Signal::new("strategy-c", "ETH-USD", dec!(-2))
        .with_alpha(dec!(-0.01))
        .with_confidence(dec!(0.8));

    aggregator.update_signal(signal_a);
    aggregator.update_signal(signal_b);
    aggregator.update_signal(signal_c);

    // === Step 2: Compute Targets ===
    let targets = aggregator.compute_targets();
    assert_eq!(targets.len(), 2); // BTC-USD and ETH-USD

    let btc_target = targets
        .iter()
        .find(|t| t.instrument_id == "BTC-USD")
        .unwrap();
    let eth_target = targets
        .iter()
        .find(|t| t.instrument_id == "ETH-USD")
        .unwrap();

    // BTC: weighted average of 5 and 3 by confidence (0.9 and 0.7)
    // Weight A = 0.9 / 1.6 = 0.5625
    // Weight B = 0.7 / 1.6 = 0.4375
    // Target = 5 * 0.5625 + 3 * 0.4375 = 2.8125 + 1.3125 = 4.125
    assert!(btc_target.target_position > dec!(4) && btc_target.target_position < dec!(4.2));
    assert_eq!(btc_target.contributing_signals.len(), 2);

    // ETH: single signal, should be -2
    assert_eq!(eth_target.target_position, dec!(-2));

    println!("BTC target: {}", btc_target.target_position);
    println!("ETH target: {}", eth_target.target_position);

    // === Step 3: Risk Check ===
    let prices: HashMap<String, Decimal> = [
        ("BTC-USD".to_string(), dec!(50000)),
        ("ETH-USD".to_string(), dec!(3000)),
    ]
    .into_iter()
    .collect();

    let btc_result =
        RiskValidator::validate(btc_target, &risk_params, &position_tracker, &prices, None);
    assert!(btc_result.passed);
    let btc_adjusted = btc_result.adjusted_target.unwrap();
    assert!(btc_adjusted.target_position <= dec!(10)); // Within limit

    let eth_result =
        RiskValidator::validate(eth_target, &risk_params, &position_tracker, &prices, None);
    assert!(eth_result.passed);

    // === Step 4: Execution Planning ===
    let market_data: HashMap<String, MarketSnapshot> = [
        (
            "BTC-USD".to_string(),
            MarketSnapshot::new("BTC-USD").with_bbo(dec!(49990), dec!(50010)),
        ),
        (
            "ETH-USD".to_string(),
            MarketSnapshot::new("ETH-USD").with_bbo(dec!(2995), dec!(3005)),
        ),
    ]
    .into_iter()
    .collect();

    let btc_plan = execution_planner
        .plan_execution(&btc_adjusted, &position_tracker, &market_data["BTC-USD"])
        .unwrap();

    assert_eq!(btc_plan.delta, btc_adjusted.target_position); // Going from 0 to target
    assert!(!btc_plan.orders.is_empty());
    println!(
        "BTC execution: {} orders, total qty {}",
        btc_plan.orders.len(),
        btc_plan.total_quantity()
    );

    for order in &btc_plan.orders {
        println!(
            "  Order {}: {:?} {} @ {:?}",
            order.client_order_id, order.side, order.quantity, order.price
        );
    }

    // === Step 5: Simulate Fills and Position Tracking ===
    // Register orders
    for order in &btc_plan.orders {
        // Get strategy from attribution
        if let Some((strategy_id, _)) = order.strategy_attribution.first() {
            position_tracker.register_order(&order.client_order_id, strategy_id);
        }
    }

    // Simulate fill for first order
    let fill = Fill {
        order_id: "exch-1".to_string(),
        client_order_id: btc_plan.orders[0].client_order_id.clone(),
        instrument_id: "BTC-USD".to_string(),
        side: FillSide::Buy,
        quantity: btc_plan.orders[0].quantity,
        price: dec!(50000),
        fee: dec!(5.0),
        fee_currency: "USD".to_string(),
        timestamp: chrono::Utc::now(),
    };

    let attribution = position_tracker.process_fill(&fill).unwrap();
    println!(
        "Fill attributed to {}, new position: {}",
        attribution.strategy_id, attribution.new_position
    );

    // Check positions
    let net_pos = position_tracker.portfolio_position("BTC-USD").unwrap();
    assert_eq!(net_pos.quantity, fill.quantity);

    // === Step 6: PnL Attribution ===
    let mark_prices: HashMap<String, Decimal> = [
        ("BTC-USD".to_string(), dec!(51000)), // Price went up!
    ]
    .into_iter()
    .collect();

    let pnl = position_tracker.pnl_by_strategy(&mark_prices);
    for (strategy_id, strategy_pnl) in &pnl {
        println!(
            "Strategy {}: realized={} unrealized={} fees={} net={}",
            strategy_id,
            strategy_pnl.realized_pnl,
            strategy_pnl.unrealized_pnl,
            strategy_pnl.total_fees,
            strategy_pnl.net_pnl()
        );
    }

    println!("\nIntegration test passed!");
}

/// Test opposing signals netting out
#[test]
fn test_opposing_strategies() {
    let mut aggregator = SignalAggregator::new(AggregatorConfig {
        weighting: WeightingMethod::Average,
        ..Default::default()
    });

    // Two strategies with opposing views
    aggregator.update_signal(Signal::new("bull", "BTC-USD", dec!(10)));
    aggregator.update_signal(Signal::new("bear", "BTC-USD", dec!(-10)));

    let targets = aggregator.compute_targets();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_position, dec!(0)); // Net flat

    println!("Opposing strategies net to flat position");
}

/// Test risk rejection stops execution
#[test]
fn test_risk_rejection() {
    let mut aggregator = SignalAggregator::new(AggregatorConfig::default());

    // Risk parameters with daily loss limit breached
    let risk_params = TradingRiskParameters {
        drawdown: DrawdownLimits {
            max_daily_loss: dec!(100),
            daily_limit_breached: true, // Limit already breached
            current_daily_pnl: dec!(-200),
            ..Default::default()
        },
        ..Default::default()
    };

    aggregator.update_signal(Signal::new("strategy", "BTC-USD", dec!(1)));
    let targets = aggregator.compute_targets();

    let position_tracker = PositionTracker::new();
    let prices = HashMap::new();

    let result =
        RiskValidator::validate(&targets[0], &risk_params, &position_tracker, &prices, None);
    assert!(!result.passed);

    println!("Risk validator correctly rejected trade after daily loss limit breached");
}

/// Test multi-strategy fill attribution
#[test]
fn test_multi_strategy_attribution() {
    let mut position_tracker = PositionTracker::new();

    // Two strategies, same instrument
    position_tracker.register_order("order-a1", "strategy-a");
    position_tracker.register_order("order-b1", "strategy-b");

    // Strategy A buys 1 BTC @ 50000
    position_tracker.process_fill(&Fill {
        order_id: "exch-1".to_string(),
        client_order_id: "order-a1".to_string(),
        instrument_id: "BTC-USD".to_string(),
        side: FillSide::Buy,
        quantity: dec!(1),
        price: dec!(50000),
        fee: dec!(5),
        fee_currency: "USD".to_string(),
        timestamp: chrono::Utc::now(),
    });

    // Strategy B sells 0.5 BTC @ 51000
    position_tracker.process_fill(&Fill {
        order_id: "exch-2".to_string(),
        client_order_id: "order-b1".to_string(),
        instrument_id: "BTC-USD".to_string(),
        side: FillSide::Sell,
        quantity: dec!(0.5),
        price: dec!(51000),
        fee: dec!(2.5),
        fee_currency: "USD".to_string(),
        timestamp: chrono::Utc::now(),
    });

    // Check individual strategy positions
    let pos_a = position_tracker
        .strategy_position("strategy-a", "BTC-USD")
        .unwrap();
    assert_eq!(pos_a.quantity, dec!(1)); // Long 1

    let pos_b = position_tracker
        .strategy_position("strategy-b", "BTC-USD")
        .unwrap();
    assert_eq!(pos_b.quantity, dec!(-0.5)); // Short 0.5

    // Check net portfolio position
    let net = position_tracker.portfolio_position("BTC-USD").unwrap();
    assert_eq!(net.quantity, dec!(0.5)); // Net long 0.5

    println!("Strategy A position: {}", pos_a.quantity);
    println!("Strategy B position: {}", pos_b.quantity);
    println!("Net portfolio position: {}", net.quantity);
}

/// Test urgency affects execution
#[test]
fn test_urgency_execution() {
    let mut planner = ExecutionPlanner::new(ExecutionConfig::default());
    let positions = PositionTracker::new();

    let market = MarketSnapshot::new("BTC-USD").with_bbo(dec!(50000), dec!(50010));

    // Passive order - should be behind best bid
    let passive_target = create_target("BTC-USD", dec!(1), Urgency::Passive);
    let passive_plan = planner
        .plan_execution(&passive_target, &positions, &market)
        .unwrap();
    assert!(passive_plan.orders[0].price.unwrap() < dec!(50000));

    // Immediate order - should be market order
    let immediate_target = create_target("BTC-USD", dec!(1), Urgency::Immediate);
    let immediate_plan = planner
        .plan_execution(&immediate_target, &positions, &market)
        .unwrap();
    assert!(immediate_plan.orders[0].price.is_none()); // Market order

    println!("Passive price: {:?}", passive_plan.orders[0].price);
    println!("Immediate: market order (no price)");
}

// Helper function
fn create_target(
    instrument: &str,
    position: Decimal,
    urgency: Urgency,
) -> athena_order_manager::aggregator::PortfolioTarget {
    athena_order_manager::aggregator::PortfolioTarget {
        instrument_id: instrument.to_string(),
        target_position: position,
        combined_alpha: None,
        combined_confidence: Decimal::ONE,
        urgency,
        stop_loss: None,
        take_profit: None,
        contributing_signals: vec![],
        timestamp: chrono::Utc::now(),
    }
}
