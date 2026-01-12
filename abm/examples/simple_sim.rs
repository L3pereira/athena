//! Simple simulation example with 2 agents

use abm::{
    MockFeed, MockFeedConfig, SimulationConfig, SimulationRunner,
    agents::{MomentumConfig, MomentumTrader, NoiseTrader, NoiseTraderConfig},
};

fn main() {
    println!("=== ABM Simulation Test ===\n");

    // Create simulation config
    let config = SimulationConfig {
        symbol: "BTC-USDT".to_string(),
        num_ticks: 500,
        tick_interval_ms: 100,
        seed: Some(42),
        verbose: false,
        ..Default::default()
    };

    // Create reference feed (simulates Binance)
    let feed = MockFeed::with_config(MockFeedConfig {
        seed: Some(42),
        ..Default::default()
    });

    // Create simulation runner
    let mut runner = SimulationRunner::new(config, feed);

    // Add agents
    println!("Adding agents:");

    // Noise trader - random trades
    let noise = NoiseTrader::new(
        "noise-1",
        NoiseTraderConfig {
            trade_probability: 0.3,
            order_size: 10_000_000, // 0.1 units
            use_market_orders: true,
            seed: Some(123),
        },
    );
    println!("  - NoiseTrader (30% trade probability)");
    runner.add_agent(Box::new(noise));

    // Momentum trader - follows trends
    let momentum = MomentumTrader::new(
        "momentum-1",
        MomentumConfig {
            lookback: 20,
            threshold_bps: 10.0,
            order_size: 50_000_000, // 0.5 units
            max_position: 200_000_000,
        },
    );
    println!("  - MomentumTrader (20 tick lookback, 10 bps threshold)");
    runner.add_agent(Box::new(momentum));

    println!("\nRunning simulation for 500 ticks...\n");

    // Run simulation
    let metrics = runner.run();

    // Print results
    println!("=== Results ===");
    println!("Total ticks:   {}", metrics.total_ticks);
    println!("Total orders:  {}", metrics.total_orders);
    println!("Total fills:   {}", metrics.total_fills);
    println!("Total volume:  {}", metrics.total_volume);
    println!("Avg spread:    {:.2} bps", metrics.avg_spread_bps);
    println!("Price vol:     {:.4}%", metrics.price_volatility * 100.0);

    println!("\nOrders by agent type:");
    for (agent_type, count) in &metrics.orders_by_type {
        println!("  {}: {}", agent_type, count);
    }

    println!("\nP&L by agent type:");
    for (agent_type, pnl) in &metrics.pnl_by_type {
        // Convert from raw (8 decimals) to readable
        let pnl_readable = *pnl as f64 / 1e8;
        println!("  {}: {:.4}", agent_type, pnl_readable);
    }

    println!("\nSimulation completed successfully!");
}
