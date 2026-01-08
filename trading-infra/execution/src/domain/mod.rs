//! Execution Domain Types
//!
//! Core value objects for execution and impact modeling.

mod impact;
mod market_state;
mod schedule;

pub use impact::{FullImpact, Impact};
pub use market_state::{MarketConditions, MarketState};
pub use schedule::{Adjustment, ExecutionSchedule, Slice};
