//! Simulator exchange adapter
//!
//! Connects the exchange-sim crate to the gateway transport layer.

mod gateway_in;
mod gateway_out;

pub use gateway_in::{OrderResponseParams, SimulatorGatewayIn};
pub use gateway_out::SimulatorGatewayOut;
