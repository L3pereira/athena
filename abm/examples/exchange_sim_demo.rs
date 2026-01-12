//! Exchange-sim integration demo
//!
//! Demonstrates agents trading against each other using the real
//! exchange-sim matching engine with price-time priority.

use abm::{
    AsyncSimulationConfig, AsyncSimulationRunner, MockFeed, MockFeedConfig,
    agents::{MomentumConfig, MomentumTrader, NoiseTrader, NoiseTraderConfig},
};
use trading_core::{Price, Quantity};

#[tokio::main]
async fn main() {
    println!("=== Exchange-sim Integration Demo ===\n");
    println!("Agents trading against each other with real order matching.\n");

    // Create simulation config
    let config = AsyncSimulationConfig {
        symbol: "BTCUSDT".to_string(),
        num_ticks: 500,
        tick_interval_ms: 100,
        initial_mid_price: Price::from_int(50_000),
        initial_spread_bps: 10, // 10 bps = 0.1%
        initial_levels: 5,
        initial_level_size: Quantity::from_raw(1_00000000), // 1 BTC per level
        initial_btc: 10_00000000,                           // 10 BTC per agent
        initial_usdt: 500_000_00000000,                     // 500k USDT per agent
        seed: Some(42),
        verbose: false,
    };

    // Create reference feed (simulates Binance price)
    let feed = MockFeed::with_config(MockFeedConfig {
        seed: Some(42),
        initial_price: 50_000_00000000, // $50,000 with 8 decimals
        ..Default::default()
    });

    // Create simulation runner
    let mut runner = AsyncSimulationRunner::new(config, feed);

    // Add agents
    println!("Adding agents:");

    // Noise traders - random trades to create volume
    for i in 0..3 {
        let noise = NoiseTrader::new(
            format!("noise-{}", i),
            NoiseTraderConfig {
                trade_probability: 0.4,
                order_size: 5_000_000,   // 0.05 BTC
                use_market_orders: true, // Take liquidity
                seed: Some(100 + i as u64),
            },
        );
        println!(
            "  - NoiseTrader-{} (40% trade probability, market orders)",
            i
        );
        runner.add_agent(Box::new(noise));
    }

    // Momentum traders - follow trends
    for i in 0..2 {
        let momentum = MomentumTrader::new(
            format!("momentum-{}", i),
            MomentumConfig {
                lookback: 15,
                threshold_bps: 8.0,
                order_size: 10_000_000,    // 0.1 BTC
                max_position: 100_000_000, // 1 BTC max position
            },
        );
        println!(
            "  - MomentumTrader-{} (15 tick lookback, 8 bps threshold)",
            i
        );
        runner.add_agent(Box::new(momentum));
    }

    println!("\nRunning simulation for 500 ticks with exchange-sim matching...\n");

    // Run simulation
    match runner.run().await {
        Ok(metrics) => {
            println!("=== Results ===");
            println!("Total ticks:    {}", metrics.total_ticks);
            println!("Total orders:   {}", metrics.total_orders);
            println!("Total fills:    {}", metrics.total_fills);
            println!("Rejected:       {}", metrics.total_rejected);
            println!("Total volume:   {} (raw units)", metrics.total_volume);
            println!("Volume (BTC):   {:.4}", metrics.total_volume as f64 / 1e8);
            println!("Avg spread:     {:.2} bps", metrics.avg_spread_bps);
            println!("Price vol:      {:.4}%", metrics.price_volatility * 100.0);

            println!("\nOrders by agent type:");
            for (agent_type, count) in &metrics.orders_by_type {
                println!("  {}: {}", agent_type, count);
            }

            println!("\nFills by agent type:");
            for (agent_type, count) in &metrics.fills_by_type {
                println!("  {}: {}", agent_type, count);
            }

            println!("\nP&L by agent type (in USDT):");
            for (agent_type, pnl) in &metrics.pnl_by_type {
                // Convert from raw (8 decimals) to readable
                let pnl_readable = *pnl as f64 / 1e8;
                println!("  {}: {:.4}", agent_type, pnl_readable);
            }

            let fill_rate = if metrics.total_orders > 0 {
                metrics.total_fills as f64 / metrics.total_orders as f64 * 100.0
            } else {
                0.0
            };
            println!(
                "\nFill rate: {:.1}% ({} fills / {} orders)",
                fill_rate, metrics.total_fills, metrics.total_orders
            );

            println!("\nSimulation completed successfully!");
        }
        Err(e) => {
            eprintln!("Simulation failed: {}", e);
        }
    }
}
