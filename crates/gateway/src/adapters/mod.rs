//! Exchange adapters
//!
//! Adapters normalize external exchange data to internal format (Gateway In)
//! and convert internal orders to exchange-specific format (Gateway Out).

pub mod simulator;

pub use simulator::{SimulatorGatewayIn, SimulatorGatewayOut};
