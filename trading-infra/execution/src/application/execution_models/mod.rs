//! Execution Models
//!
//! Implementations of execution scheduling algorithms.
//!
//! From docs Section 4: Execution Models
//! - TWAP: Trade evenly over time
//! - VWAP: Trade proportionally to volume
//! - Implementation Shortfall: Almgren-Chriss optimal schedule
//! - Adaptive: Real-time adjustment based on conditions

mod adaptive;
mod implementation_shortfall;
mod protocol;
mod twap;
mod vwap;

pub use adaptive::AdaptiveModel;
pub use implementation_shortfall::ImplementationShortfallModel;
pub use protocol::ExecutionModel;
pub use twap::TwapModel;
pub use vwap::VwapModel;
