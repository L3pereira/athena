//! Athena Order Manager
//!
//! The Order Manager sits between strategies and the gateway, responsible for:
//! - **Signal Aggregation**: Combines signals from multiple strategies into a portfolio
//! - **Position Tracking**: Tracks fills, positions, and PnL per strategy
//! - **Risk Management**: Pre-trade risk checks, position limits, exposure limits
//! - **Execution Planning**: Converts portfolio deltas into executable orders
//!
//! ## Architecture
//!
//! ```text
//! Strategies ──► Signals ──► ┌────────────────────────────────────────┐
//!                            │            Order Manager               │
//!                            │  ┌─────────────────────────────────┐   │
//!                            │  │   Signal Aggregator             │   │
//!                            │  │   - Net positions per instrument│   │
//!                            │  │   - Weight by alpha/confidence  │   │
//!                            │  └───────────────┬─────────────────┘   │
//!                            │                  │ Target Portfolio    │
//!                            │  ┌───────────────▼─────────────────┐   │
//!                            │  │   Risk Manager                  │   │
//!                            │  │   - Position limits             │   │
//!                            │  │   - Exposure limits             │   │
//!                            │  │   - Drawdown checks             │   │
//!                            │  └───────────────┬─────────────────┘   │
//!                            │                  │ Risk-adjusted       │
//!                            │  ┌───────────────▼─────────────────┐   │
//!                            │  │   Execution Planner             │   │
//!                            │  │   - Delta from current position │   │
//!                            │  │   - Order slicing               │   │
//!                            │  │   - Urgency-based execution     │   │
//!                            │  └───────────────┬─────────────────┘   │
//!                            │                  │ Orders              │
//!                            └──────────────────┼─────────────────────┘
//!                                               │
//! Gateway Out ◄──────────────  Order Requests ◄─┘
//!
//! Gateway In ─────────────► Fills ─────────► Position Tracker
//!                                                   │
//!                                                   ▼
//!                                           PnL Attribution ──► Strategies
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use athena_order_manager::{OrderManager, Signal};
//!
//! let mut manager = OrderManager::new(config);
//!
//! // Strategy emits signal
//! let signal = Signal::new("mm-btc", "BTC-USD", dec!(1.5))
//!     .with_alpha(dec!(0.02))
//!     .with_confidence(dec!(0.8));
//!
//! // Order manager processes signal
//! let orders = manager.process_signal(signal).await;
//! ```

pub mod aggregator;
pub mod error;
pub mod execution;
pub mod position;
pub mod risk;
pub mod signal;

// Re-export main types
pub use aggregator::{PortfolioTarget, SignalAggregator};
pub use error::{Error, Result};
pub use execution::{
    CostEstimator, CostEstimatorConfig, ExecutionCostEstimate, ExecutionOrder, ExecutionPlan,
    ExecutionPlanner, MarketSnapshot,
};
pub use position::{Fill, PositionTracker, StrategyPosition};
pub use risk::{RiskResult, RiskValidator, RiskViolation};
pub use signal::{Signal, Urgency};
