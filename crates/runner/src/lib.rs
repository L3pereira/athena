//! Athena Runner - Multi-Agent Trading Simulation
//!
//! Orchestrates the full trading system with multiple agents:
//!
//! - **Bootstrap**: Capital allocation and simulation setup
//! - **Event Feed**: External information source (fair value, sentiment)
//! - **Agent Runner**: Runs individual trading strategies
//! - **Simulation**: Full orchestration of all components
//!
//! ## Architecture
//!
//! ```text
//!                         ┌─────────────────┐
//!                         │   Event Feed    │
//!                         │ (Price/News)    │
//!                         └────────┬────────┘
//!                                  │ events
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        TRADING AGENTS                           │
//! │                                                                 │
//! │  ┌──────────────────┐              ┌──────────────────┐        │
//! │  │  Market Maker    │              │  Mean Reversion  │        │
//! │  │  Agent           │              │  Taker Agent     │        │
//! │  └────────┬─────────┘              └────────┬─────────┘        │
//! │           │ signals                         │ signals          │
//! │           └──────────────┬──────────────────┘                  │
//! │                          ▼                                      │
//! │              ┌───────────────────────┐                         │
//! │              │    Order Manager      │                         │
//! │              └───────────┬───────────┘                         │
//! └──────────────────────────┼──────────────────────────────────────┘
//!                            │ orders
//!                            ▼
//!               ┌───────────────────────┐
//!               │       Gateway         │
//!               └───────────┬───────────┘
//!                           │
//!                           ▼
//!               ┌───────────────────────┐
//!               │     Exchange-Sim      │
//!               └───────────────────────┘
//! ```

pub mod agent;
pub mod bootstrap;
pub mod event_feed;
pub mod simulation;

// Re-export main types
pub use agent::{AgentConfig, AgentOrder, AgentRunner};
pub use bootstrap::{AgentAccount, AgentType, BootstrapConfig, SimulationBootstrap};
pub use event_feed::{EventFeedConfig, EventFeedSimulator};
pub use simulation::{SimulationConfig, SimulationResults, TradingSimulation};

// Re-export MarketEvent from strategy crate for convenience
pub use athena_strategy::MarketEvent;
