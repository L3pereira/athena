//! Regime Detection
//!
//! Implements detection of market regime and shifts.
//!
//! From docs/reflexive_market_architecture.md:
//! - Moment-based detection: Statistical moment deviation > 2σ
//! - Shift detection: Persistent deviation = confirmed shift

mod moment_based;
mod protocol;
mod shift_detector;

pub use moment_based::MomentBasedDetector;
pub use protocol::RegimeDetector;
pub use shift_detector::{RegimeShiftDetector, ShiftDetectorConfig};
