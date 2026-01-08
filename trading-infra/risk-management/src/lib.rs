//! Risk Management, Regime Detection, and Market Making
//!
//! This crate provides the reflexive market architecture from docs/reflexive_market_architecture.md:
//!
//! # Regime Detection
//!
//! - [`MomentBasedDetector`](application::regime_detection::MomentBasedDetector): Statistical moment deviation
//! - [`RegimeShiftDetector`](application::regime_detection::RegimeShiftDetector): Persistent deviation = confirmed shift
//!
//! # Market Making (Avellaneda-Stoikov)
//!
//! - [`AvellanedaStoikov`](application::market_making::AvellanedaStoikov): Optimal quoting with reservation price
//! - [`InventorySkew`](application::market_making::InventorySkew): Position-based quote adjustment
//! - [`ToxicFlowDetector`](application::market_making::ToxicFlowDetector): VPIN, OFI signals
//!
//! # Reflexive Loop
//!
//! The core concept from Soros: trades can shift market structure permanently.
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │                                             │
//! │  Trader → Trade → Impact → Structure Change │
//! │     ↑                           │           │
//! │     └───── New Regime ←─────────┘           │
//! │                                             │
//! └─────────────────────────────────────────────┘
//! ```

pub mod application;
pub mod domain;
pub mod infrastructure;

// Re-export commonly used types
pub use application::manipulation::{
    MomentumIgnitionDetector, QuoteStuffingDetector, SpoofingDetector,
};
pub use application::market_making::{
    AvellanedaStoikov, InventorySkew, QuotingModel, ToxicFlowDetector,
};
pub use application::reflexive::{CircuitBreaker, CircuitState, ReflexiveEvent, ReflexiveLoop};
pub use application::regime_detection::{MomentBasedDetector, RegimeDetector, RegimeShiftDetector};
pub use domain::{
    Inventory, MarketRegime, OrderbookMoments, Quote, RegimeShift, ToxicityLevel, ToxicityMetrics,
};
