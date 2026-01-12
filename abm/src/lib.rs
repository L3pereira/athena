//! Agent-Based Model (ABM) for market simulation
//!
//! This crate provides tools for simulating market dynamics with multiple agents.
//!
//! # Components
//!
//! - **Synthetic Orderbook Generator**: Generate orderbooks from statistical moments
//! - **Agents**: Profit-seeking agents (DMM, Arbitrageur, Momentum, etc.)
//!
//! # Agent Philosophy
//!
//! All agents are profit-seeking. Market regime emerges from their interactions:
//! - DMM widens spread when volatility high → "stressed" regime
//! - DMM skews quotes when inventory builds → "trending" regime
//! - DMM goes one-sided when exhausted → "crisis" regime

pub mod application;
pub mod domain;
pub mod infrastructure;

// Re-export modules from application for convenience
pub use application::agents;
pub use application::simulation;

// Python bindings (optional, enabled with "python" feature)
#[cfg(feature = "python")]
mod python;

#[cfg(feature = "python")]
pub use python::*;

// Re-export key types at crate root
pub use application::generators::{GeneratedOrderbook, SyntheticOrderbookGenerator};
pub use domain::{MarketStructureState, NUM_LEVELS, OrderbookMoments};

// Re-export agent types
pub use application::agents::dmm::{DMMAgent, DMMConfig};
pub use application::agents::{Agent, AgentAction, AgentId, MarketState};

// Re-export infrastructure
pub use infrastructure::{ExchangeAdapter, MockFeed, MockFeedConfig, Scenario};

// Re-export simulation
pub use application::simulation::{
    AsyncSimulationConfig, AsyncSimulationMetrics, AsyncSimulationRunner, SimulationConfig,
    SimulationMetrics, SimulationRunner, SimulationState,
};
