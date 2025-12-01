//! Athena Ports
//!
//! Port definitions (traits) for the Athena trading system.
//! These define the boundaries between domain logic and infrastructure.

mod clock;
mod error;
mod matching;
mod risk;

pub use clock::Clock;
pub use error::{MatchingError, MatchingResult};
pub use matching::MatchingAlgorithm;
pub use risk::{LiquidationOrder, RiskCheckResult, RiskConfig, RiskError, RiskManager, RiskResult};
