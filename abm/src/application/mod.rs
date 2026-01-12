//! Application layer: Use cases and orchestration
//!
//! Contains:
//! - **agents**: Profit-seeking agent implementations (DMM, Arbitrageur, etc.)
//! - **generators**: Synthetic orderbook generation from statistical moments
//! - **simulation**: SimulationRunner that coordinates agents

pub mod agents;
pub mod generators;
pub mod simulation;
