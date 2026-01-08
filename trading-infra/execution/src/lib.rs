//! Execution and Impact Models
//!
//! This crate provides execution scheduling and market impact estimation for trading systems.
//!
//! # Impact Models
//!
//! - [`LinearImpact`](application::impact_models::LinearImpact): Kyle's lambda, Impact = λ × Q
//! - [`SquareRootImpact`](application::impact_models::SquareRootImpact): Almgren, Impact = σ × (Q/V)^0.5
//! - [`ObizhaevaWangImpact`](application::impact_models::ObizhaevaWangImpact): Transient + permanent with resilience
//! - [`FullImpactModel`](application::impact_models::FullImpactModel): Multi-dimensional L2 structure impact
//!
//! # Execution Models
//!
//! - [`TwapModel`](application::execution_models::TwapModel): Time-weighted average price
//! - [`VwapModel`](application::execution_models::VwapModel): Volume-weighted average price
//! - [`ImplementationShortfallModel`](application::execution_models::ImplementationShortfallModel): Almgren-Chriss optimal
//! - [`AdaptiveModel`](application::execution_models::AdaptiveModel): Real-time adaptive execution
//!
//! # Example
//!
//! ```rust,no_run
//! use execution::application::impact_models::{ImpactModel, SquareRootImpact};
//! use execution::application::execution_models::{ExecutionModel, TwapModel};
//! use execution::domain::MarketState;
//! use trading_core::{Quantity, Side, Price};
//! use chrono::Duration;
//!
//! // Estimate impact
//! let impact_model = SquareRootImpact::almgren();
//! let market = MarketState {
//!     mid_price: Price::from_int(100),
//!     volatility: 0.2,
//!     daily_volume: Quantity::from_int(100_000),
//!     ..Default::default()
//! };
//! let impact = impact_model.estimate(Quantity::from_int(1000), Side::Buy, &market);
//!
//! // Create execution schedule
//! let exec_model = TwapModel::with_slices(10);
//! let schedule = exec_model.compute_schedule(
//!     Quantity::from_int(1000),
//!     Duration::hours(1),
//!     &market,
//! );
//! ```

pub mod application;
pub mod domain;
pub mod infrastructure;

// Re-export commonly used types
pub use application::execution_models::{
    AdaptiveModel, ExecutionModel, ImplementationShortfallModel, TwapModel, VwapModel,
};
pub use application::impact_models::{
    FullImpactModel, ImpactModel, LinearImpact, ObizhaevaWangImpact, SquareRootImpact,
};
pub use domain::{
    Adjustment, ExecutionSchedule, FullImpact, Impact, MarketConditions, MarketState, Slice,
};
