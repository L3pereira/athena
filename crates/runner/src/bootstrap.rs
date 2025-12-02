//! Bootstrap - Capital allocation and simulation setup
//!
//! Handles initial setup of the simulation:
//! - Registering instruments on the exchange
//! - Allocating capital to agents
//! - Ensuring market makers have liquidity first

use athena_core::{MarginAccount, MarginMode};
use exchange_sim::{Exchange, model::ExchangeMessage};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::mpsc::{self, Receiver};
use uuid::Uuid;

/// Type of trading agent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    /// Market maker - provides liquidity
    MarketMaker,
    /// Taker - consumes liquidity (informed or noise)
    Taker,
}

/// Agent account configuration
#[derive(Debug, Clone)]
pub struct AgentAccount {
    /// Unique agent identifier
    pub agent_id: String,
    /// Type of agent
    pub agent_type: AgentType,
    /// Initial capital allocation
    pub initial_capital: Decimal,
    /// Instruments this agent trades
    pub instruments: Vec<String>,
    /// Exchange account ID (assigned after registration)
    pub account_id: Option<Uuid>,
}

impl AgentAccount {
    /// Create a new market maker agent account
    pub fn market_maker(agent_id: &str, capital: Decimal, instruments: Vec<String>) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            agent_type: AgentType::MarketMaker,
            initial_capital: capital,
            instruments,
            account_id: None,
        }
    }

    /// Create a new taker agent account
    pub fn taker(agent_id: &str, capital: Decimal, instruments: Vec<String>) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            agent_type: AgentType::Taker,
            initial_capital: capital,
            instruments,
            account_id: None,
        }
    }
}

/// Bootstrap configuration
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Trading symbols (instruments)
    pub symbols: Vec<String>,
    /// Agent accounts to create
    pub agents: Vec<AgentAccount>,
    /// Heartbeat interval in ms
    pub heartbeat_interval_ms: u64,
    /// Channel capacity
    pub channel_capacity: usize,
    /// Default matching algorithm
    pub matching_algorithm: String,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            symbols: vec!["BTC-USD".to_string()],
            agents: vec![
                AgentAccount::market_maker(
                    "mm-agent",
                    dec!(1_000_000),
                    vec!["BTC-USD".to_string()],
                ),
                AgentAccount::taker("taker-agent", dec!(500_000), vec!["BTC-USD".to_string()]),
            ],
            heartbeat_interval_ms: 1000,
            channel_capacity: 10000,
            matching_algorithm: "price-time".to_string(),
        }
    }
}

/// Simulation bootstrap - sets up exchange and agents
pub struct SimulationBootstrap {
    /// The exchange instance
    pub exchange: Exchange,
    /// Receiver for exchange messages (trades, order updates, etc.)
    pub message_rx: Receiver<ExchangeMessage>,
    /// Registered agents with their account IDs
    pub agents: Vec<AgentAccount>,
}

impl SimulationBootstrap {
    /// Create a new bootstrap with default configuration
    pub async fn new() -> Result<Self, exchange_sim::error::ExchangeError> {
        Self::with_config(BootstrapConfig::default()).await
    }

    /// Create bootstrap with custom configuration
    pub async fn with_config(
        config: BootstrapConfig,
    ) -> Result<Self, exchange_sim::error::ExchangeError> {
        // Create channel for exchange messages
        let (client_tx, message_rx) = mpsc::channel::<ExchangeMessage>(config.channel_capacity);

        // Create exchange
        let exchange = Exchange::new(
            config.symbols.clone(),
            client_tx,
            config.heartbeat_interval_ms,
            config.channel_capacity,
            config.matching_algorithm,
        )
        .await?;

        // Register agents - Market Makers FIRST to ensure liquidity
        let mut agents = config.agents;

        // Sort by agent type - market makers first
        agents.sort_by_key(|a| match a.agent_type {
            AgentType::MarketMaker => 0,
            AgentType::Taker => 1,
        });

        // Register accounts
        for agent in &mut agents {
            let account = MarginAccount::new(
                agent.agent_id.clone(), // owner_id
                agent.initial_capital,  // initial_balance
                MarginMode::Cross,      // margin_mode
                dec!(0.10),             // initial_margin_rate (10x leverage)
                dec!(0.05),             // maintenance_margin_rate
            );

            let account_id = exchange.register_account(account).await;
            agent.account_id = Some(account_id);

            log::info!(
                "Registered {:?} agent '{}' with account {} and capital {}",
                agent.agent_type,
                agent.agent_id,
                account_id,
                agent.initial_capital
            );
        }

        Ok(Self {
            exchange,
            message_rx,
            agents,
        })
    }

    /// Get agent account by agent ID
    pub fn get_agent(&self, agent_id: &str) -> Option<&AgentAccount> {
        self.agents.iter().find(|a| a.agent_id == agent_id)
    }

    /// Get all market maker agents
    pub fn market_makers(&self) -> Vec<&AgentAccount> {
        self.agents
            .iter()
            .filter(|a| a.agent_type == AgentType::MarketMaker)
            .collect()
    }

    /// Get all taker agents
    pub fn takers(&self) -> Vec<&AgentAccount> {
        self.agents
            .iter()
            .filter(|a| a.agent_type == AgentType::Taker)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bootstrap_creates_agents() {
        let bootstrap = SimulationBootstrap::new().await.unwrap();

        assert_eq!(bootstrap.agents.len(), 2);
        assert_eq!(bootstrap.market_makers().len(), 1);
        assert_eq!(bootstrap.takers().len(), 1);

        // All agents should have account IDs
        for agent in &bootstrap.agents {
            assert!(agent.account_id.is_some());
        }
    }

    #[tokio::test]
    async fn test_market_makers_registered_first() {
        let bootstrap = SimulationBootstrap::new().await.unwrap();

        // First agent should be market maker
        assert_eq!(bootstrap.agents[0].agent_type, AgentType::MarketMaker);
    }
}
