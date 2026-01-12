//! Simulation Framework
//!
//! Provides the event loop and coordination for running agent-based market simulations.
//!
//! # Architecture
//!
//! The simulation runner coordinates:
//! - Market state generation (synthetic orderbook)
//! - Agent decision making (on_tick)
//! - Order matching simulation
//! - Fill processing (on_fill)
//! - Metrics collection
//!
//! # Runners
//!
//! - [`SimulationRunner`]: Synchronous runner with simplified fill simulation
//! - [`AsyncSimulationRunner`]: Async runner using exchange-sim for real matching

mod async_runner;
mod runner;

pub use async_runner::{AsyncSimulationConfig, AsyncSimulationMetrics, AsyncSimulationRunner};
pub use runner::{
    SimulationConfig, SimulationMetrics, SimulationRunner, SimulationState, TickResult,
};
