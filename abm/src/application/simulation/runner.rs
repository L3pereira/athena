//! Simulation Runner
//!
//! The core event loop that coordinates agents and market state.

use crate::application::agents::{Agent, AgentAction, Fill, MarketState, OrderType};
use crate::application::generators::SyntheticOrderbookGenerator;
use crate::domain::OrderbookMoments;
use crate::infrastructure::MockFeed;
use risk_management::ReferenceFeed;
use std::collections::HashMap;
use trading_core::{Price, Quantity, Side};

/// Configuration for the simulation
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    /// Symbol being traded
    pub symbol: String,
    /// Number of ticks to simulate
    pub num_ticks: u64,
    /// Tick interval in milliseconds
    pub tick_interval_ms: u64,
    /// Fee rate in basis points
    pub fee_rate_bps: f64,
    /// Initial moments for orderbook generation
    pub initial_moments: OrderbookMoments,
    /// Random seed for determinism
    pub seed: Option<u64>,
    /// Enable verbose logging
    pub verbose: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            symbol: "BTC-USDT".to_string(),
            num_ticks: 1000,
            tick_interval_ms: 100,
            fee_rate_bps: 1.0, // 1 bps fee
            initial_moments: OrderbookMoments::default_normal(),
            seed: Some(42),
            verbose: false,
        }
    }
}

/// Result of a single tick
#[derive(Debug, Clone)]
pub struct TickResult {
    /// Tick number
    pub tick: u64,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Mid price at this tick
    pub mid_price: Price,
    /// Spread in basis points
    pub spread_bps: f64,
    /// Number of orders submitted this tick
    pub orders_submitted: usize,
    /// Number of fills this tick
    pub fills_executed: usize,
    /// Total volume traded this tick
    pub volume: i64,
}

/// Simulation metrics aggregated over the run
#[derive(Debug, Clone, Default)]
pub struct SimulationMetrics {
    /// Total ticks processed
    pub total_ticks: u64,
    /// Total orders submitted
    pub total_orders: u64,
    /// Total fills executed
    pub total_fills: u64,
    /// Total volume traded (raw units)
    pub total_volume: i64,
    /// P&L by agent type
    pub pnl_by_type: HashMap<String, i64>,
    /// Orders by agent type
    pub orders_by_type: HashMap<String, u64>,
    /// Average spread (bps)
    pub avg_spread_bps: f64,
    /// Average price
    pub avg_price: f64,
    /// Price volatility
    pub price_volatility: f64,
}

/// Current state of the simulation
#[derive(Debug, Clone)]
pub struct SimulationState {
    /// Current tick number
    pub tick: u64,
    /// Current timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Current market state
    pub market: MarketState,
    /// Accumulated metrics
    pub metrics: SimulationMetrics,
}

/// Pending order in the simulation
#[derive(Debug, Clone)]
struct PendingOrder {
    agent_idx: usize,
    client_order_id: u64,
    side: Side,
    price: Price,
    quantity: Quantity,
    order_type: OrderType,
}

/// The simulation runner coordinates agents and market state
pub struct SimulationRunner {
    config: SimulationConfig,
    agents: Vec<Box<dyn Agent>>,
    orderbook_gen: SyntheticOrderbookGenerator,
    reference_feed: MockFeed,
    state: SimulationState,
    price_history: Vec<i64>,
}

impl SimulationRunner {
    /// Create a new simulation runner
    pub fn new(config: SimulationConfig, reference_feed: MockFeed) -> Self {
        let seed = config.seed.unwrap_or(0);
        let orderbook_gen = SyntheticOrderbookGenerator::new(config.initial_moments.clone(), seed);

        let market = MarketState::empty(&config.symbol);

        Self {
            config,
            agents: Vec::new(),
            orderbook_gen,
            reference_feed,
            state: SimulationState {
                tick: 0,
                timestamp_ms: 0,
                market,
                metrics: SimulationMetrics::default(),
            },
            price_history: Vec::with_capacity(1000),
        }
    }

    /// Add an agent to the simulation
    pub fn add_agent(&mut self, agent: Box<dyn Agent>) {
        self.agents.push(agent);
    }

    /// Get current simulation state
    pub fn state(&self) -> &SimulationState {
        &self.state
    }

    /// Get reference to agents
    pub fn agents(&self) -> &[Box<dyn Agent>] {
        &self.agents
    }

    /// Run the full simulation
    pub fn run(&mut self) -> SimulationMetrics {
        let num_ticks = self.config.num_ticks;

        for _ in 0..num_ticks {
            self.tick();
        }

        self.finalize_metrics();
        self.state.metrics.clone()
    }

    /// Run a single tick of the simulation
    pub fn tick(&mut self) -> TickResult {
        // 1. Update reference feed
        self.reference_feed.tick();

        // 2. Get mid price from reference and generate orderbook
        let mid_price = self.reference_feed.mid_price();
        let orderbook = self.orderbook_gen.generate(mid_price);

        // 3. Update market state
        self.state.market.update_from_orderbook(&orderbook);
        self.state.market.timestamp_ms = self.state.timestamp_ms;

        let spread_bps = self.state.market.spread_bps();

        // Track price for volatility calculation
        self.price_history.push(mid_price.raw());

        // 4. Collect actions from all agents
        let mut pending_orders: Vec<PendingOrder> = Vec::new();

        for (agent_idx, agent) in self.agents.iter_mut().enumerate() {
            let actions = agent.on_tick(&self.state.market);

            for action in actions {
                match action {
                    AgentAction::SubmitOrder {
                        client_order_id,
                        side,
                        price,
                        quantity,
                        order_type,
                        ..
                    } => {
                        pending_orders.push(PendingOrder {
                            agent_idx,
                            client_order_id,
                            side,
                            price,
                            quantity,
                            order_type,
                        });
                    }
                    AgentAction::CancelOrder { .. } | AgentAction::CancelAll => {
                        // Simplified: instant cancel, no pending orders tracked
                    }
                    AgentAction::ModifyOrder { .. } => {
                        // Simplified: treat as cancel + resubmit, not implemented yet
                    }
                    AgentAction::NoOp => {}
                }
            }
        }

        // 5. Simple order matching simulation
        let orders_submitted = pending_orders.len();
        let mut fills_executed = 0;
        let mut volume = 0i64;

        for order in pending_orders {
            if let Some(fill) = self.simulate_fill(&order) {
                fills_executed += 1;
                volume += fill.signed_qty.abs();

                // Dispatch fill to agent
                self.agents[order.agent_idx].on_fill(&fill);

                // Track metrics by agent type
                let agent_type = self.agents[order.agent_idx].agent_type().to_string();
                *self
                    .state
                    .metrics
                    .orders_by_type
                    .entry(agent_type)
                    .or_insert(0) += 1;
            }
        }

        // 6. Update metrics
        self.state.metrics.total_ticks += 1;
        self.state.metrics.total_orders += orders_submitted as u64;
        self.state.metrics.total_fills += fills_executed as u64;
        self.state.metrics.total_volume += volume;

        // Track running averages
        let tick = self.state.tick as f64 + 1.0;
        self.state.metrics.avg_spread_bps =
            (self.state.metrics.avg_spread_bps * (tick - 1.0) + spread_bps) / tick;
        self.state.metrics.avg_price =
            (self.state.metrics.avg_price * (tick - 1.0) + mid_price.raw() as f64) / tick;

        let result = TickResult {
            tick: self.state.tick,
            timestamp_ms: self.state.timestamp_ms,
            mid_price,
            spread_bps,
            orders_submitted,
            fills_executed,
            volume,
        };

        // Advance time
        self.state.tick += 1;
        self.state.timestamp_ms += self.config.tick_interval_ms;

        if self.config.verbose && self.state.tick % 100 == 0 {
            eprintln!(
                "Tick {}: price={}, spread={:.1}bps, orders={}, fills={}",
                self.state.tick,
                mid_price.raw(),
                spread_bps,
                orders_submitted,
                fills_executed
            );
        }

        result
    }

    /// Simulate fill for an order
    fn simulate_fill(&self, order: &PendingOrder) -> Option<Fill> {
        let market = &self.state.market;

        // Determine fill price based on order type and side
        let fill_price = match order.order_type {
            OrderType::Market => {
                // Market orders get filled at current BBO
                match order.side {
                    Side::Buy => market.bbo.ask_price,
                    Side::Sell => market.bbo.bid_price,
                }
            }
            OrderType::Limit => {
                // Limit orders check if they can be filled
                match order.side {
                    Side::Buy => {
                        if order.price >= market.bbo.ask_price {
                            market.bbo.ask_price
                        } else {
                            return None; // Would rest on book
                        }
                    }
                    Side::Sell => {
                        if order.price <= market.bbo.bid_price {
                            market.bbo.bid_price
                        } else {
                            return None; // Would rest on book
                        }
                    }
                }
            }
            OrderType::PostOnly => {
                // Post-only orders only fill if they would provide liquidity
                // They are rejected if they would cross the spread
                match order.side {
                    Side::Buy => {
                        if order.price >= market.bbo.ask_price {
                            return None; // Would cross, rejected
                        }
                        return None; // Would rest on book, not filled immediately
                    }
                    Side::Sell => {
                        if order.price <= market.bbo.bid_price {
                            return None; // Would cross, rejected
                        }
                        return None; // Would rest on book, not filled immediately
                    }
                }
            }
        };

        // Calculate fee
        let notional = (fill_price.raw() as f64) * (order.quantity.raw() as f64) / 1e8;
        let fee = (notional * self.config.fee_rate_bps / 10_000.0) as i64;

        // Create signed quantity
        let signed_qty = match order.side {
            Side::Buy => order.quantity.raw(),
            Side::Sell => -order.quantity.raw(),
        };

        Some(Fill {
            order_id: order.client_order_id,
            price: fill_price,
            signed_qty,
            fee,
            timestamp_ms: self.state.timestamp_ms,
        })
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
            self.state.metrics.price_volatility = variance.sqrt() / mean;
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
            *self
                .state
                .metrics
                .pnl_by_type
                .entry(agent_type)
                .or_insert(0) += total_pnl;
        }
    }

    /// Reset the simulation state
    pub fn reset(&mut self) {
        self.state.tick = 0;
        self.state.timestamp_ms = 0;
        self.state.market = MarketState::empty(&self.config.symbol);
        self.state.metrics = SimulationMetrics::default();
        self.price_history.clear();

        // Reset orderbook generator with fresh seed
        let seed = self.config.seed.unwrap_or(0);
        self.orderbook_gen =
            SyntheticOrderbookGenerator::new(self.config.initial_moments.clone(), seed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agents::{
        MomentumConfig, MomentumTrader, NoiseTrader, NoiseTraderConfig,
    };
    use crate::infrastructure::MockFeedConfig;

    #[test]
    fn test_simulation_runs() {
        let config = SimulationConfig {
            num_ticks: 100,
            seed: Some(42),
            verbose: false,
            ..Default::default()
        };

        let feed_config = MockFeedConfig {
            seed: Some(42),
            ..Default::default()
        };
        let feed = MockFeed::with_config(feed_config);

        let mut runner = SimulationRunner::new(config, feed);

        // Add a noise trader
        let noise_config = NoiseTraderConfig {
            trade_probability: 0.5,
            seed: Some(123),
            ..Default::default()
        };
        runner.add_agent(Box::new(NoiseTrader::new("noise-1", noise_config)));

        let metrics = runner.run();

        assert_eq!(metrics.total_ticks, 100);
        assert!(metrics.total_orders > 0);
        assert!(metrics.avg_spread_bps > 0.0);
    }

    #[test]
    fn test_multiple_agents() {
        let config = SimulationConfig {
            num_ticks: 200,
            seed: Some(42),
            ..Default::default()
        };

        let feed = MockFeed::with_config(MockFeedConfig {
            seed: Some(42),
            ..Default::default()
        });

        let mut runner = SimulationRunner::new(config, feed);

        // Add noise traders
        for i in 0..3 {
            let noise_config = NoiseTraderConfig {
                trade_probability: 0.3,
                seed: Some(100 + i),
                ..Default::default()
            };
            runner.add_agent(Box::new(NoiseTrader::new(
                format!("noise-{}", i),
                noise_config,
            )));
        }

        // Add momentum trader
        let mom_config = MomentumConfig {
            lookback: 10,
            threshold_bps: 5.0,
            ..Default::default()
        };
        runner.add_agent(Box::new(MomentumTrader::new("momentum-1", mom_config)));

        let metrics = runner.run();

        assert_eq!(metrics.total_ticks, 200);
        assert!(metrics.orders_by_type.contains_key("NoiseTrader"));
    }

    #[test]
    fn test_tick_by_tick() {
        let config = SimulationConfig {
            num_ticks: 10,
            seed: Some(42),
            ..Default::default()
        };

        let feed = MockFeed::new();
        let mut runner = SimulationRunner::new(config, feed);

        // Run 5 ticks manually
        for i in 0..5 {
            let result = runner.tick();
            assert_eq!(result.tick, i);
            assert!(result.mid_price.raw() > 0);
        }

        assert_eq!(runner.state().tick, 5);
    }

    #[test]
    fn test_deterministic() {
        let config = SimulationConfig {
            num_ticks: 50,
            seed: Some(999),
            ..Default::default()
        };

        // Run 1
        let feed1 = MockFeed::with_config(MockFeedConfig {
            seed: Some(999),
            ..Default::default()
        });
        let mut runner1 = SimulationRunner::new(config.clone(), feed1);
        runner1.add_agent(Box::new(NoiseTrader::new(
            "noise",
            NoiseTraderConfig {
                seed: Some(111),
                trade_probability: 0.5,
                ..Default::default()
            },
        )));
        let metrics1 = runner1.run();

        // Run 2
        let feed2 = MockFeed::with_config(MockFeedConfig {
            seed: Some(999),
            ..Default::default()
        });
        let mut runner2 = SimulationRunner::new(config, feed2);
        runner2.add_agent(Box::new(NoiseTrader::new(
            "noise",
            NoiseTraderConfig {
                seed: Some(111),
                trade_probability: 0.5,
                ..Default::default()
            },
        )));
        let metrics2 = runner2.run();

        // Should be identical
        assert_eq!(metrics1.total_orders, metrics2.total_orders);
        assert_eq!(metrics1.total_fills, metrics2.total_fills);
        assert_eq!(metrics1.total_volume, metrics2.total_volume);
    }

    #[test]
    fn test_reset() {
        let config = SimulationConfig {
            num_ticks: 50,
            seed: Some(42),
            ..Default::default()
        };

        let feed = MockFeed::new();
        let mut runner = SimulationRunner::new(config, feed);

        runner.run();
        assert_eq!(runner.state().tick, 50);

        runner.reset();
        assert_eq!(runner.state().tick, 0);
        assert_eq!(runner.state().metrics.total_ticks, 0);
    }
}
