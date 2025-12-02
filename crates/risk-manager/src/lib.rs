//! Athena Trading Risk Manager
//!
//! Active risk management for trading systems. Unlike exchange-level risk
//! (margin, leverage, liquidation), this handles:
//!
//! - **Portfolio Limits**: Position and exposure limits across strategies
//! - **Strategy Limits**: Per-strategy risk budgets
//! - **Market Surveillance**: Book quality, manipulation detection
//! - **Drawdown Control**: Daily loss and peak-to-trough limits
//! - **Risk Parameters**: Published to Order Manager for validation
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Trading Risk Manager                       │
//! │                                                             │
//! │  Market Data ───► Surveillance ───► Market Quality Score   │
//! │                                                             │
//! │  PnL Updates ───► Drawdown Monitor ───► Trading Enabled?   │
//! │                                                             │
//! │  Config ───────► Limits ────────────► Risk Parameters      │
//! │                                                             │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//!                      TradingRiskParameters
//!                              │
//!                              ▼
//!                       Order Manager
//! ```
//!
//! ## Separation from Exchange Risk
//!
//! | Exchange Risk (athena-risk) | Trading Risk (this crate) |
//! |----------------------------|---------------------------|
//! | Margin calculations | Portfolio limits |
//! | Leverage limits | Strategy risk budgets |
//! | Liquidation triggers | Drawdown control |
//! | Account-level | System-level |
//! | Passive (validates) | Active (monitors & publishes) |

pub mod manager;
pub mod parameters;
pub mod surveillance;

// Re-export main types
pub use manager::TradingRiskManager;
pub use parameters::{
    CostLimits, DrawdownLimits, InstrumentLimits, MarketQualityLimit, StrategyLimits,
    TradingRiskParameters,
};
pub use surveillance::{BookQualityMetrics, MarketQuality, MarketSurveillance, SurveillanceAlert};
