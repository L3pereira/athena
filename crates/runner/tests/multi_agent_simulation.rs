//! Multi-Agent Simulation Integration Test
//!
//! Tests the full trading simulation with:
//! - Market Maker Agent (provides liquidity)
//! - Mean Reversion Taker Agent (informed trader)
//! - Event Feed (fair value signals)
//! - Exchange simulation

use athena_runner::{
    bootstrap::{AgentType, BootstrapConfig, SimulationBootstrap},
    event_feed::{EventFeedConfig, EventFeedSimulator},
    simulation::{SimulationConfig, TradingSimulation},
};
use athena_strategy::{
    BasicMarketMaker, MarketMakerConfig, MeanReversionConfig, MeanReversionTaker, Strategy,
};
use rust_decimal_macros::dec;
use std::time::Duration;

/// Test that simulation can be created and run
#[tokio::test]
async fn test_simulation_runs() {
    let config = SimulationConfig {
        duration: Duration::from_millis(100),
        event_interval_ms: 50,
        ..Default::default()
    };

    let sim = TradingSimulation::with_config(config).await.unwrap();
    let results = sim.run().await;

    assert!(results.success, "Simulation should complete successfully");
}

/// Test bootstrap creates agents correctly
#[tokio::test]
async fn test_bootstrap_creates_agents() {
    let config = BootstrapConfig::default();
    let bootstrap = SimulationBootstrap::with_config(config).await.unwrap();

    // Should have market maker and taker accounts
    assert_eq!(bootstrap.agents.len(), 2);

    let mm = bootstrap
        .agents
        .iter()
        .find(|a| a.agent_type == AgentType::MarketMaker);
    let taker = bootstrap
        .agents
        .iter()
        .find(|a| a.agent_type == AgentType::Taker);

    assert!(mm.is_some(), "Should have market maker agent");
    assert!(taker.is_some(), "Should have taker agent");

    // Market maker should have account ID (registered with exchange)
    assert!(
        mm.unwrap().account_id.is_some(),
        "Market maker should have exchange account"
    );
    assert!(
        taker.unwrap().account_id.is_some(),
        "Taker should have exchange account"
    );
}

/// Test event feed generates events
#[test]
fn test_event_feed_generates_events() {
    let config = EventFeedConfig::default();
    let mut feed = EventFeedSimulator::with_seed(config, 42);

    // Generate 10 events
    for _ in 0..10 {
        let event = feed.next_event();
        // Should be valid event type
        assert!(!event.instrument_id().is_empty());
    }
}

/// Test event feed fair value tracking
#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_event_feed_fair_value_tracking() {
    let mut config = EventFeedConfig::default();
    config.sentiment_probability = 0.0; // Only fair value events
    config.initial_fair_values.clear();
    config
        .initial_fair_values
        .insert("TEST".to_string(), dec!(100));

    let mut feed = EventFeedSimulator::with_seed(config, 42);

    let initial = feed.fair_value("TEST").unwrap();
    assert_eq!(initial, dec!(100));

    // Generate events (fair values only)
    for _ in 0..50 {
        feed.next_event();
    }

    let final_val = feed.fair_value("TEST").unwrap();
    // Price should have moved (random walk)
    // Allow for some movement but stay in reasonable range
    assert!(final_val > dec!(80) && final_val < dec!(120));
}

/// Test market maker strategy configuration
#[test]
fn test_market_maker_config() {
    let config = MarketMakerConfig {
        instrument_id: "BTC-USD".to_string(),
        spread_bps: dec!(20),
        quote_size: dec!(0.1),
        max_position: dec!(5),
        skew_factor: dec!(2),
        tick_size: dec!(0.01),
        requote_threshold: dec!(10),
    };

    let mm = BasicMarketMaker::new(config);
    assert_eq!(mm.name(), "BasicMarketMaker");
}

/// Test mean reversion taker strategy configuration
#[test]
fn test_mean_reversion_config() {
    let config = MeanReversionConfig {
        instrument_id: "BTC-USD".to_string(),
        entry_threshold_bps: dec!(100), // 1% deviation to enter
        exit_threshold_bps: dec!(20),   // 0.2% to exit
        trade_size: dec!(0.05),
        max_position: dec!(2),
    };

    let taker = MeanReversionTaker::new(config);
    assert_eq!(taker.name(), "MeanReversionTaker");
}

/// Test simulation with custom event feed
#[tokio::test]
async fn test_simulation_with_event_feed() {
    let event_config = EventFeedConfig {
        price_volatility: dec!(0.001), // Higher volatility for testing
        sentiment_probability: 0.1,    // 10% sentiment events
        ..Default::default()
    };

    let config = SimulationConfig {
        event_feed: event_config,
        duration: Duration::from_millis(200),
        event_interval_ms: 20,
        ..Default::default()
    };

    let sim = TradingSimulation::with_config(config).await.unwrap();
    let results = sim.run().await;

    assert!(results.success, "Simulation should complete successfully");
}

/// Test that agent orders are tracked
#[tokio::test]
async fn test_simulation_tracks_orders() {
    let config = SimulationConfig {
        duration: Duration::from_millis(500),
        event_interval_ms: 50,
        ..Default::default()
    };

    let sim = TradingSimulation::with_config(config).await.unwrap();
    let results = sim.run().await;

    assert!(results.success, "Simulation should complete successfully");
    // In a longer simulation, we should see orders being placed
    // For now, just verify the counters are accessible
    println!("Total orders: {}", results.total_orders);
    println!("Total trades: {}", results.total_trades);
}
