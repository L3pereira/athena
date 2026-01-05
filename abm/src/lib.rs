//! Agent-Based Model (ABM) for market simulation
//!
//! This crate provides tools for simulating market dynamics with multiple agents.
//! The first component is a synthetic orderbook generator based on statistical moments.

pub mod application;
pub mod domain;

// Python bindings (optional, enabled with "python" feature)
#[cfg(feature = "python")]
mod python;

#[cfg(feature = "python")]
pub use python::*;

// Re-export key types at crate root
pub use application::generators::{GeneratedOrderbook, SyntheticOrderbookGenerator};
pub use domain::{MarketStructureState, NUM_LEVELS, OrderbookMoments};
