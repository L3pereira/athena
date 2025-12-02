//! Simulation - Full trading system orchestration
//!
//! Ties together all components:
//! - Exchange simulation
//! - Event feed
//! - Multiple trading agents
//! - Market data distribution
//! - Order routing

use crate::{
    agent::{AgentConfig, AgentOrder, AgentRunner},
    bootstrap::{AgentAccount, AgentType, BootstrapConfig, SimulationBootstrap},
    event_feed::{EventFeedConfig, EventFeedSimulator},
};
use athena_core::{InstrumentId, Order, OrderType, Side, TimeInForce};
use athena_gateway::messages::{
    market_data::{BookLevel, OrderBookUpdate},
    order::{OrderResponse, OrderSide, TimeInForceWire},
};
use athena_strategy::{BasicMarketMaker, MarketMakerConfig, Strategy};
use exchange_sim::{Exchange, model::ExchangeMessage};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, broadcast, mpsc};

/// Order book state: (bids, asks) where each is a Vec of (price, quantity)
type OrderBookState = (Vec<(Decimal, Decimal)>, Vec<(Decimal, Decimal)>);

/// Simulation configuration
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    /// Bootstrap configuration
    pub bootstrap: BootstrapConfig,
    /// Event feed configuration
    pub event_feed: EventFeedConfig,
    /// Event feed tick interval (ms)
    pub event_interval_ms: u64,
    /// Total simulation duration
    pub duration: Duration,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            bootstrap: BootstrapConfig::default(),
            event_feed: EventFeedConfig::default(),
            event_interval_ms: 500,
            duration: Duration::from_secs(10),
        }
    }
}

/// Simulation results
#[derive(Debug, Clone, Default)]
pub struct SimulationResults {
    /// Total trades executed
    pub total_trades: u64,
    /// Trades by agent
    pub trades_by_agent: HashMap<String, u64>,
    /// Final positions by agent
    pub positions_by_agent: HashMap<String, HashMap<String, Decimal>>,
    /// PnL by agent
    pub pnl_by_agent: HashMap<String, Decimal>,
    /// Total orders submitted
    pub total_orders: u64,
    /// Orders by agent
    pub orders_by_agent: HashMap<String, u64>,
    /// Whether simulation completed successfully
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
}

/// Full trading simulation
pub struct TradingSimulation {
    /// Configuration
    config: SimulationConfig,
    /// Exchange instance
    exchange: Arc<Exchange>,
    /// Exchange message receiver
    exchange_message_rx: mpsc::Receiver<ExchangeMessage>,
    /// Agent accounts
    agents: Vec<AgentAccount>,
    /// Market data broadcaster (agents subscribe to this)
    md_tx: broadcast::Sender<OrderBookUpdate>,
    /// Event feed simulator
    event_feed: EventFeedSimulator,
    /// Order channel (agents -> exchange bridge)
    order_tx: mpsc::Sender<AgentOrder>,
    order_rx: mpsc::Receiver<AgentOrder>,
    /// Order response channels (per agent)
    response_channels: HashMap<String, mpsc::Sender<OrderResponse>>,
    /// Results accumulator
    results: Arc<RwLock<SimulationResults>>,
}

impl TradingSimulation {
    /// Create a new simulation with default configuration
    pub async fn new() -> Result<Self, exchange_sim::error::ExchangeError> {
        Self::with_config(SimulationConfig::default()).await
    }

    /// Create a new simulation with custom configuration
    pub async fn with_config(
        config: SimulationConfig,
    ) -> Result<Self, exchange_sim::error::ExchangeError> {
        // Bootstrap exchange and agents
        let bootstrap = SimulationBootstrap::with_config(config.bootstrap.clone()).await?;

        // Create channels
        let (md_tx, _) = broadcast::channel(1000);
        let (order_tx, order_rx) = mpsc::channel(1000);

        // Create event feed
        let event_feed = EventFeedSimulator::new(config.event_feed.clone());

        Ok(Self {
            config,
            exchange: Arc::new(bootstrap.exchange),
            exchange_message_rx: bootstrap.message_rx,
            agents: bootstrap.agents,
            md_tx,
            event_feed,
            order_tx,
            order_rx,
            response_channels: HashMap::new(),
            results: Arc::new(RwLock::new(SimulationResults::default())),
        })
    }

    /// Create and register an agent with a strategy
    pub fn create_agent<S: Strategy + 'static>(
        &mut self,
        agent_id: &str,
        strategy: S,
    ) -> Option<AgentRunner<S>> {
        let agent_account = self.agents.iter().find(|a| a.agent_id == agent_id)?;
        let account_id = agent_account.account_id?;

        // Create response channel for this agent
        let (response_tx, response_rx) = mpsc::channel(100);
        self.response_channels
            .insert(agent_id.to_string(), response_tx);

        // Subscribe to market data
        let md_rx = self.md_tx.subscribe();

        // Subscribe to event feed if taker
        let event_rx = if agent_account.agent_type == AgentType::Taker {
            Some(self.event_feed.subscribe())
        } else {
            None
        };

        let config = AgentConfig {
            agent_id: agent_id.to_string(),
            account_id,
            instruments: agent_account.instruments.clone(),
            uses_event_feed: event_rx.is_some(),
        };

        Some(AgentRunner::new(
            config,
            strategy,
            md_rx,
            event_rx,
            self.order_tx.clone(),
            response_rx,
        ))
    }

    /// Create default market maker agent
    pub fn create_market_maker(&mut self) -> Option<AgentRunner<BasicMarketMaker>> {
        let mm_config = MarketMakerConfig {
            instrument_id: "BTC-USD".to_string(),
            spread_bps: dec!(10),   // 10 bps spread
            quote_size: dec!(0.1),  // Quote 0.1 BTC
            max_position: dec!(10), // Max 10 BTC
            skew_factor: dec!(0.1), // Inventory skew
            ..Default::default()
        };

        let strategy = BasicMarketMaker::new(mm_config);
        self.create_agent("mm-agent", strategy)
    }

    /// Run the exchange bridge (order routing from agents to exchange)
    async fn run_exchange_bridge(
        exchange: Arc<Exchange>,
        mut order_rx: mpsc::Receiver<AgentOrder>,
        response_channels: HashMap<String, mpsc::Sender<OrderResponse>>,
        results: Arc<RwLock<SimulationResults>>,
    ) {
        log::info!("Exchange bridge started");

        while let Some(agent_order) = order_rx.recv().await {
            let agent_id = agent_order.agent_id.clone();

            // Convert OrderRequest to Exchange Order
            let time_in_force = match agent_order.request.time_in_force {
                TimeInForceWire::Gtc => TimeInForce::GTC,
                TimeInForceWire::Ioc => TimeInForce::IOC,
                TimeInForceWire::Fok => TimeInForce::FOK,
            };

            let order = Order::new(
                InstrumentId::new(&agent_order.request.instrument_id),
                match agent_order.request.side {
                    OrderSide::Buy => Side::Buy,
                    OrderSide::Sell => Side::Sell,
                },
                if agent_order.request.price.is_some() {
                    OrderType::Limit
                } else {
                    OrderType::Market
                },
                agent_order.request.quantity,
                agent_order.request.price,
                None, // stop_price
                time_in_force,
            );

            log::debug!(
                "[Bridge] Submitting order from {}: {:?} {} {} @ {:?}",
                agent_id,
                order.side,
                order.quantity,
                order.instrument_id,
                agent_order.request.price
            );

            // Submit to exchange
            match exchange.submit_order(order).await {
                Ok(order_id) => {
                    // Update results
                    {
                        let mut r = results.write().await;
                        r.total_orders += 1;
                        *r.orders_by_agent.entry(agent_id.clone()).or_insert(0) += 1;
                    }

                    // Send acceptance to agent
                    if let Some(tx) = response_channels.get(&agent_id) {
                        let response = OrderResponse::accepted(
                            agent_order.request.client_order_id,
                            order_id.to_string(),
                            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                        );
                        let _ = tx.send(response).await;
                    }
                }
                Err(e) => {
                    log::warn!("[Bridge] Order rejected: {}", e);

                    // Send rejection to agent
                    if let Some(tx) = response_channels.get(&agent_id) {
                        let response = OrderResponse::rejected(
                            agent_order.request.client_order_id,
                            e.to_string(),
                            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                        );
                        let _ = tx.send(response).await;
                    }
                }
            }
        }

        log::info!("Exchange bridge stopped");
    }

    /// Run the exchange message processor (converts exchange events to market data)
    async fn run_message_processor(
        mut exchange_rx: mpsc::Receiver<ExchangeMessage>,
        md_tx: broadcast::Sender<OrderBookUpdate>,
        results: Arc<RwLock<SimulationResults>>,
    ) {
        log::info!("Exchange message processor started");

        // Track order book state per instrument
        let order_books: HashMap<String, OrderBookState> = HashMap::new();
        let mut sequence: u64 = 0;

        while let Some(msg) = exchange_rx.recv().await {
            match msg {
                ExchangeMessage::Trade(trade) => {
                    log::debug!(
                        "Trade: {} {} @ {} (qty {})",
                        trade.symbol(),
                        trade.price,
                        trade.quantity,
                        trade.id
                    );

                    // Update results
                    {
                        let mut r = results.write().await;
                        r.total_trades += 1;
                    }
                }
                ExchangeMessage::OrderUpdate {
                    order_id,
                    status,
                    filled_qty,
                    symbol: _,
                } => {
                    log::debug!(
                        "Order update: {} - {:?} (filled: {})",
                        order_id,
                        status,
                        filled_qty
                    );
                }
                ExchangeMessage::Heartbeat(time) => {
                    // On heartbeat, publish current order book state
                    for (symbol, (bids, asks)) in &order_books {
                        sequence += 1;
                        let bid_levels: Vec<BookLevel> = bids
                            .iter()
                            .map(|(price, qty)| BookLevel::new(*price, *qty))
                            .collect();
                        let ask_levels: Vec<BookLevel> = asks
                            .iter()
                            .map(|(price, qty)| BookLevel::new(*price, *qty))
                            .collect();

                        let update = OrderBookUpdate::snapshot(
                            symbol.clone(),
                            bid_levels,
                            ask_levels,
                            sequence,
                            time.timestamp_nanos_opt().unwrap_or(0),
                        );

                        if md_tx.send(update).is_err() {
                            log::trace!("No market data subscribers");
                        }
                    }
                }
                _ => {}
            }
        }

        log::info!("Exchange message processor stopped");
    }

    /// Run event feed generator
    async fn run_event_feed(mut event_feed: EventFeedSimulator, interval_ms: u64) {
        log::info!("Event feed started ({}ms interval)", interval_ms);
        event_feed.run(interval_ms).await;
    }

    /// Run the full simulation
    pub async fn run(mut self) -> SimulationResults {
        log::info!("Starting trading simulation...");

        let duration = self.config.duration;
        let event_interval = self.config.event_interval_ms;

        // Create market maker BEFORE moving out components
        let mm_agent = self.create_market_maker();

        // Take ownership of components
        let exchange = self.exchange.clone();
        let md_tx = self.md_tx.clone();
        let results = self.results.clone();
        let response_channels = std::mem::take(&mut self.response_channels);
        let order_rx = self.order_rx;
        let exchange_message_rx = self.exchange_message_rx;

        // Spawn exchange bridge
        let bridge_handle = tokio::spawn(Self::run_exchange_bridge(
            exchange.clone(),
            order_rx,
            response_channels,
            results.clone(),
        ));

        // Spawn exchange message processor
        let processor_handle = tokio::spawn(Self::run_message_processor(
            exchange_message_rx,
            md_tx.clone(),
            results.clone(),
        ));

        // Spawn event feed
        let event_feed = self.event_feed;
        let event_handle = tokio::spawn(Self::run_event_feed(event_feed, event_interval));

        // Spawn agents
        let agent_handles = vec![
            mm_agent.map(|agent| tokio::spawn(agent.run())),
            // Add more agents here
        ];

        // Run for specified duration
        log::info!("Simulation running for {:?}...", duration);
        tokio::time::sleep(duration).await;

        // Shutdown - drop all handles
        log::info!("Simulation complete, shutting down...");

        // Cancel all tasks
        bridge_handle.abort();
        processor_handle.abort();
        event_handle.abort();
        for handle in agent_handles.into_iter().flatten() {
            handle.abort();
        }

        // Collect results
        let mut final_results = results.read().await.clone();
        final_results.success = true;

        log::info!(
            "Simulation finished: {} orders, {} trades",
            final_results.total_orders,
            final_results.total_trades
        );

        final_results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulation_creation() {
        let sim = TradingSimulation::new().await.unwrap();

        assert_eq!(sim.agents.len(), 2);
        assert!(sim.agents.iter().any(|a| a.agent_id == "mm-agent"));
        assert!(sim.agents.iter().any(|a| a.agent_id == "taker-agent"));
    }

    #[tokio::test]
    async fn test_simulation_short_run() {
        let config = SimulationConfig {
            duration: Duration::from_millis(100),
            ..Default::default()
        };

        let sim = TradingSimulation::with_config(config).await.unwrap();
        let results = sim.run().await;

        assert!(results.success);
    }
}
