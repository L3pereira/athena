//! Market Manipulation Detection
//!
//! From docs Section 9: Market Manipulation: Techniques, Detection, and Defense
//!
//! Detection methods for:
//! - Quote stuffing: Massive order bursts with high cancel rate
//! - Layering/Spoofing: Large fake orders at consecutive levels
//! - Momentum ignition: Aggressive initiating trades with reversal
//! - Pinging: Small probing orders

mod momentum_ignition;
mod quote_stuffing;
mod spoofing;

pub use momentum_ignition::MomentumIgnitionDetector;
pub use quote_stuffing::QuoteStuffingDetector;
pub use spoofing::SpoofingDetector;
