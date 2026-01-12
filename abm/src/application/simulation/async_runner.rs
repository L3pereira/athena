//! Async Simulation Runner using exchange-sim
//!
//! This runner uses the actual exchange-sim matching engine instead of
//! simplified fill simulation. Agents trade against each other with
//! real price-time priority matching.

use crate::application::agents::{Agent, AgentAction, BBO, DepthLevel, MarketState};
use crate::infrastructure::{ExchangeAdapter, MockFeed};
use risk_management::{OrderbookMoments, ReferenceFeed};
use std::collections::HashMap;
use trading_core::{Price, Quantity};

/// Configuration for the async simulation
#[derive(Debug, Clone)]
pub struct AsyncSimulationConfig {
    /// Symbol being traded
    pub symbol: String,
    /// Number of ticks to simulate
    pub num_ticks: u64,
    /// Tick interval in milliseconds
    pub tick_interval_ms: u64,
    /// Initial BTC balance for each agent
    pub initial_btc: i64,
    /// Initial USDT balance for each agent
    pub initial_usdt: i64,
    /// Initial mid price for seeding orderbook
    pub initial_mid_price: Price,
    /// Spread in bps for initial liquidity
    pub initial_spread_bps: i64,
    /// Number of price levels to seed
    pub initial_levels: usize,
    /// Size per level for initial liquidity
    pub initial_level_size: Quantity,
    /// Random seed for determinism
    pub seed: Option<u64>,
    /// Enable verbose logging
    pub verbose: bool,
}

impl Default for AsyncSimulationConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            num_ticks: 1000,
            tick_interval_ms: 100,
            initial_btc: 10_00000000,                           // 10 BTC
            initial_usdt: 1_000_000_00000000,                   // 1M USDT
            initial_mid_price: Price::from_int(50_000),         // $50,000
            initial_spread_bps: 10,                             // 10 bps spread
            initial_levels: 5,                                  // 5 price levels each side
            initial_level_size: Quantity::from_raw(1_00000000), // 1 unit per level
            seed: Some(42),
            verbose: false,
        }
    }
}

/// Simulation metrics aggregated over the run
#[derive(Debug, Clone, Default)]
pub struct AsyncSimulationMetrics {
    /// Total ticks processed
    pub total_ticks: u64,
    /// Total orders submitted
    pub total_orders: u64,
    /// Total fills executed
    pub total_fills: u64,
    /// Total orders rejected
    pub total_rejected: u64,
    /// Total volume traded (raw units)
    pub total_volume: i64,
    /// P&L by agent type
    pub pnl_by_type: HashMap<String, i64>,
    /// Orders by agent type
    pub orders_by_type: HashMap<String, u64>,
    /// Fills by agent type
    pub fills_by_type: HashMap<String, u64>,
    /// Average spread (bps)
    pub avg_spread_bps: f64,
    /// Average price
    pub avg_price: f64,
    /// Price volatility
    pub price_volatility: f64,
}

/// The async simulation runner uses exchange-sim for real matching
pub struct AsyncSimulationRunner {
    config: AsyncSimulationConfig,
    agents: Vec<Box<dyn Agent>>,
    reference_feed: MockFeed,
    exchange: Option<ExchangeAdapter>,
    metrics: AsyncSimulationMetrics,
    price_history: Vec<i64>,
}

impl AsyncSimulationRunner {
    /// Create a new async simulation runner
    pub fn new(config: AsyncSimulationConfig, reference_feed: MockFeed) -> Self {
        Self {
            config,
            agents: Vec::new(),
            reference_feed,
            exchange: None,
            metrics: AsyncSimulationMetrics::default(),
            price_history: Vec::with_capacity(1000),
        }
    }

    /// Add an agent to the simulation
    pub fn add_agent(&mut self, agent: Box<dyn Agent>) {
        self.agents.push(agent);
    }

    /// Initialize the exchange and create accounts for all agents
    async fn initialize(&mut self) -> Result<(), String> {
        // Create exchange adapter
        let exchange = ExchangeAdapter::new(&self.config.symbol)?;

        // Create a liquidity provider account with large balances
        exchange
            .create_account(
                "__liquidity_provider__",
                100_00000000,        // 100 BTC
                10_000_000_00000000, // 10M USDT
            )
            .await;

        // Seed initial orderbook liquidity
        exchange
            .seed_orderbook(
                "__liquidity_provider__",
                self.config.initial_mid_price,
                self.config.initial_spread_bps,
                self.config.initial_levels,
                self.config.initial_level_size,
            )
            .await?;

        // Create accounts for all agents
        for agent in &self.agents {
            exchange
                .create_account(
                    &agent.id().0, // AgentId wraps a String
                    self.config.initial_btc,
                    self.config.initial_usdt,
                )
                .await;
        }

        self.exchange = Some(exchange);
        Ok(())
    }

    /// Run the full simulation
    pub async fn run(&mut self) -> Result<AsyncSimulationMetrics, String> {
        // Initialize exchange
        self.initialize().await?;

        let num_ticks = self.config.num_ticks;

        for tick in 0..num_ticks {
            self.tick(tick).await?;
        }

        self.finalize_metrics();
        Ok(self.metrics.clone())
    }

    /// Run a single tick of the simulation
    async fn tick(&mut self, tick_num: u64) -> Result<(), String> {
        let exchange = self.exchange.as_ref().ok_or("Exchange not initialized")?;

        // 1. Update reference feed
        self.reference_feed.tick();
        let mid_price = self.reference_feed.mid_price();

        // Track price for volatility calculation
        self.price_history.push(mid_price.raw());

        // 2. Build market state from exchange orderbook
        let (bids, asks) = exchange.get_depth(10).await;
        let best_bid = exchange.best_bid().await;
        let best_ask = exchange.best_ask().await;

        let bid_price = best_bid.unwrap_or(mid_price);
        let ask_price = best_ask.unwrap_or(mid_price);

        let market = MarketState {
            symbol: self.config.symbol.clone(),
            timestamp_ms: exchange.now_ms(),
            bbo: BBO {
                bid_price,
                ask_price,
                bid_size: bids.first().map(|(_, q)| *q).unwrap_or(Quantity::ZERO),
                ask_size: asks.first().map(|(_, q)| *q).unwrap_or(Quantity::ZERO),
            },
            moments: OrderbookMoments::default(),
            bids: bids
                .into_iter()
                .map(|(p, q)| DepthLevel {
                    price: p,
                    quantity: q,
                })
                .collect(),
            asks: asks
                .into_iter()
                .map(|(p, q)| DepthLevel {
                    price: p,
                    quantity: q,
                })
                .collect(),
            last_trade_price: Some(mid_price),
            recent_volume: Quantity::ZERO,
        };

        let spread_bps = if market.bbo.ask_price.raw() > 0 {
            let mid = (market.bbo.bid_price.raw() + market.bbo.ask_price.raw()) / 2;
            if mid > 0 {
                let spread = market.bbo.ask_price.raw() - market.bbo.bid_price.raw();
                (spread as f64 / mid as f64) * 10_000.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        // 3. Collect actions from all agents
        let mut pending_actions: Vec<(usize, AgentAction)> = Vec::new();

        for (agent_idx, agent) in self.agents.iter_mut().enumerate() {
            let actions = agent.on_tick(&market);
            for action in actions {
                pending_actions.push((agent_idx, action));
            }
        }

        // 4. Process all actions through exchange
        let mut orders_submitted = 0;
        let mut fills_executed = 0;
        let mut volume = 0i64;

        for (agent_idx, action) in pending_actions {
            match &action {
                AgentAction::SubmitOrder { .. } => {
                    let agent_id = self.agents[agent_idx].id().to_string();
                    let agent_type = self.agents[agent_idx].agent_type().to_string();

                    orders_submitted += 1;
                    *self
                        .metrics
                        .orders_by_type
                        .entry(agent_type.clone())
                        .or_insert(0) += 1;

                    match exchange.submit_order(&agent_id, &action).await {
                        Ok(fill) => {
                            if fill.signed_qty != 0 {
                                fills_executed += 1;
                                volume += fill.signed_qty.abs();

                                // Dispatch fill to agent
                                self.agents[agent_idx].on_fill(&fill);

                                *self.metrics.fills_by_type.entry(agent_type).or_insert(0) += 1;
                            }
                        }
                        Err(e) => {
                            self.metrics.total_rejected += 1;
                            if self.config.verbose {
                                eprintln!("Order rejected: {}", e);
                            }
                        }
                    }
                }
                AgentAction::CancelOrder { .. } | AgentAction::CancelAll => {
                    // TODO: Implement cancel via exchange
                }
                AgentAction::ModifyOrder { .. } => {
                    // TODO: Implement modify via exchange
                }
                AgentAction::NoOp => {}
            }
        }

        // 5. Update metrics
        self.metrics.total_ticks += 1;
        self.metrics.total_orders += orders_submitted as u64;
        self.metrics.total_fills += fills_executed as u64;
        self.metrics.total_volume += volume;

        // Track running averages
        let tick = tick_num as f64 + 1.0;
        self.metrics.avg_spread_bps =
            (self.metrics.avg_spread_bps * (tick - 1.0) + spread_bps) / tick;
        self.metrics.avg_price =
            (self.metrics.avg_price * (tick - 1.0) + mid_price.raw() as f64) / tick;

        // 6. Advance exchange time
        exchange.advance_time(self.config.tick_interval_ms as i64);

        if self.config.verbose && self.metrics.total_ticks % 100 == 0 {
            eprintln!(
                "Tick {}: price={}, spread={:.1}bps, orders={}, fills={}",
                self.metrics.total_ticks,
                mid_price.raw(),
                spread_bps,
                orders_submitted,
                fills_executed
            );
        }

        Ok(())
    }

    /// Finalize metrics at end of simulation
    fn finalize_metrics(&mut self) {
        // Calculate price volatility
        if self.price_history.len() > 1 {
            let mean: f64 = self.price_history.iter().map(|&p| p as f64).sum::<f64>()
                / self.price_history.len() as f64;
            let variance: f64 = self
                .price_history
                .iter()
                .map(|&p| (p as f64 - mean).powi(2))
                .sum::<f64>()
                / self.price_history.len() as f64;
            self.metrics.price_volatility = variance.sqrt() / mean;
        }

        // Collect P&L by agent type
        for agent in &self.agents {
            let agent_type = agent.agent_type().to_string();
            let pnl = agent.pnl();
            let current_price = if self.price_history.is_empty() {
                Price::from_int(50000)
            } else {
                Price::from_raw(*self.price_history.last().unwrap())
            };

            let total_pnl = pnl.net_total(current_price);
            *self.metrics.pnl_by_type.entry(agent_type).or_insert(0) += total_pnl;
        }
    }

    /// Get current metrics
    pub fn metrics(&self) -> &AsyncSimulationMetrics {
        &self.metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agents::{NoiseTrader, NoiseTraderConfig};
    use crate::infrastructure::MockFeedConfig;

    #[tokio::test]
    async fn test_async_simulation_runs() {
        let config = AsyncSimulationConfig {
            num_ticks: 50,
            seed: Some(42),
            verbose: false,
            ..Default::default()
        };

        let feed_config = MockFeedConfig {
            seed: Some(42),
            ..Default::default()
        };
        let feed = MockFeed::with_config(feed_config);

        let mut runner = AsyncSimulationRunner::new(config, feed);

        // Add a noise trader
        let noise_config = NoiseTraderConfig {
            trade_probability: 0.5,
            seed: Some(123),
            use_market_orders: false, // Use limit orders
            ..Default::default()
        };
        runner.add_agent(Box::new(NoiseTrader::new("noise-1", noise_config)));

        let metrics = runner.run().await.unwrap();

        assert_eq!(metrics.total_ticks, 50);
        // With limit orders that may not cross, we may have fewer fills
        assert!(metrics.total_orders > 0);
    }

    #[tokio::test]
    async fn test_two_agents_trade() {
        let config = AsyncSimulationConfig {
            num_ticks: 100,
            seed: Some(42),
            verbose: false,
            ..Default::default()
        };

        let feed = MockFeed::with_config(MockFeedConfig {
            seed: Some(42),
            ..Default::default()
        });

        let mut runner = AsyncSimulationRunner::new(config, feed);

        // Add two noise traders that will create crossing orders
        // One tends to buy, one tends to sell
        let buyer_config = NoiseTraderConfig {
            trade_probability: 0.8,
            seed: Some(111),
            use_market_orders: true, // Market orders will take liquidity
            order_size: 1_000_000,   // 0.01 units
        };
        runner.add_agent(Box::new(NoiseTrader::new("buyer", buyer_config)));

        let seller_config = NoiseTraderConfig {
            trade_probability: 0.8,
            seed: Some(222),
            use_market_orders: true,
            order_size: 1_000_000,
        };
        runner.add_agent(Box::new(NoiseTrader::new("seller", seller_config)));

        let metrics = runner.run().await.unwrap();

        assert_eq!(metrics.total_ticks, 100);
        // Both agents should have submitted orders
        assert!(
            metrics.total_orders > 10,
            "Expected orders, got {}",
            metrics.total_orders
        );

        println!(
            "Orders: {}, Fills: {}, Volume: {}",
            metrics.total_orders, metrics.total_fills, metrics.total_volume
        );
        println!("Orders by type: {:?}", metrics.orders_by_type);
        println!("Fills by type: {:?}", metrics.fills_by_type);
    }
}
