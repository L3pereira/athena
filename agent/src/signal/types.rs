//! Signal types and data structures
//!
//! This module re-exports types from the domain layer for backwards compatibility.
//! New code should import directly from `signal::domain`.

// Re-export all domain types for backwards compatibility
#[allow(unused_imports)]
pub use crate::signal::domain::{
    Features, Leg, Signal, SignalBuilder, SignalDirection, SignalId, StrategyId, StrategyType,
    Urgency,
};
