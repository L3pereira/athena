//! Agent Framework
//!
//! Traits and types for profit-seeking agents in the ABM simulation.
//!
//! # Agent Types
//!
//! All agents are profit-seeking. Regime dynamics emerge from their interactions:
//!
//! - **DMM**: Market maker using A-S model (spread revenue vs adverse selection)
//! - **Arbitrageur**: Profits from local/reference price discrepancy
//! - **NoiseTrader**: Random trader providing baseline volume
//! - **MomentumTrader**: Bets on trend continuation (amplifies trends)
//! - **MeanReversionTrader**: Bets on overreaction correction (dampens volatility)
//! - **InformedTrader**: Has information advantage (toxic flow for DMM)

mod action;
mod agent;
mod market_state;

pub use action::{AgentAction, OrderType, TimeInForce};
pub use agent::{Agent, AgentId, Fill, MarketEvent};
pub use market_state::{BBO, DepthLevel, MarketState};

// Agent implementations
pub mod arbitrageur;
pub mod dmm;
pub mod informed;
pub mod mean_reversion;
pub mod momentum;
pub mod noise;

// Re-export agent types and configs
pub use arbitrageur::{ArbitrageConfig, Arbitrageur};
pub use dmm::{DMMAgent, DMMConfig};
pub use informed::{InformedTrader, InformedTraderConfig};
pub use mean_reversion::{MeanReversionConfig, MeanReversionTrader};
pub use momentum::{MomentumConfig, MomentumTrader};
pub use noise::{NoiseTrader, NoiseTraderConfig};
