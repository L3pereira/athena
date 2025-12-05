//! Transaction Cost Analysis (TCA) Module
//!
//! A comprehensive, production-grade TCA system for execution cost estimation,
//! optimal scheduling, and post-trade measurement.
//!
//! # Overview
//!
//! This module implements industry-standard models from academic literature:
//!
//! - **Kyle (1985)**: Linear price impact from informed trading
//! - **Almgren-Chriss (2000)**: Optimal execution with temporary + permanent impact
//! - **Bouchaud et al.**: Empirical square-root law for market impact
//!
//! # Components
//!
//! - [`models`]: Impact model implementations
//! - [`estimator`]: Pre-trade cost estimation
//! - [`scheduler`]: Optimal execution algorithms (TWAP, VWAP, IS)
//! - [`measurement`]: Post-trade TCA measurement
//! - [`calibration`]: Parameter calibration from historical data
//! - [`benchmark`]: Execution benchmarks (arrival, VWAP, IS)
//!
//! # Example
//!
//! ```rust,ignore
//! use athena_order_manager::tca::{
//!     TcaEstimator, AlmgrenChrissParams, ExecutionScheduler,
//!     SchedulerType, TcaMeasurement,
//! };
//!
//! // Pre-trade estimation
//! let estimator = TcaEstimator::new(config);
//! let estimate = estimator.estimate_cost(&order, &market_state);
//!
//! // Generate optimal schedule
//! let scheduler = ExecutionScheduler::new(SchedulerType::ImplementationShortfall {
//!     risk_aversion: dec!(0.001),
//! });
//! let schedule = scheduler.generate_schedule(&order, &market_state);
//!
//! // Post-trade measurement
//! let measurement = TcaMeasurement::measure(&executions, &benchmarks);
//! ```

pub mod benchmark;
pub mod calibration;
pub mod estimator;
pub mod measurement;
pub mod models;
pub mod scheduler;

// Re-export main types
pub use benchmark::{Benchmark, BenchmarkType, ExecutionBenchmarks};
pub use calibration::{CalibrationConfig, CalibrationResult, ImpactCalibrator};
pub use estimator::{TcaEstimate, TcaEstimator, TcaEstimatorConfig};
pub use measurement::{ExecutionRecord, TcaMeasurement, TcaMetrics};
pub use models::{
    AlmgrenChrissModel, AlmgrenChrissParams, ImpactModel, ImpactType, KyleModel, KyleParams,
    MarketImpact, SquareRootModel, SquareRootParams,
};
pub use scheduler::{
    ExecutionSchedule, ExecutionScheduler, ScheduleSlice, SchedulerConfig, SchedulerType,
};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Market state for TCA calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketState {
    /// Instrument identifier
    pub instrument_id: String,
    /// Best bid price
    pub best_bid: Option<Decimal>,
    /// Best ask price
    pub best_ask: Option<Decimal>,
    /// Bid depth at top of book
    pub bid_depth: Option<Decimal>,
    /// Ask depth at top of book
    pub ask_depth: Option<Decimal>,
    /// Average daily volume (shares/contracts)
    pub adv: Decimal,
    /// Intraday volatility (annualized, as decimal e.g., 0.20 = 20%)
    pub volatility: Decimal,
    /// Typical bid-ask spread in basis points
    pub typical_spread_bps: Decimal,
    /// Current timestamp
    pub timestamp: DateTime<Utc>,
}

impl MarketState {
    /// Create new market state
    pub fn new(instrument_id: impl Into<String>) -> Self {
        Self {
            instrument_id: instrument_id.into(),
            best_bid: None,
            best_ask: None,
            bid_depth: None,
            ask_depth: None,
            adv: Decimal::ZERO,
            volatility: Decimal::ZERO,
            typical_spread_bps: Decimal::ZERO,
            timestamp: Utc::now(),
        }
    }

    /// Builder: set BBO
    pub fn with_bbo(mut self, bid: Decimal, ask: Decimal) -> Self {
        self.best_bid = Some(bid);
        self.best_ask = Some(ask);
        self
    }

    /// Builder: set depth
    pub fn with_depth(mut self, bid_depth: Decimal, ask_depth: Decimal) -> Self {
        self.bid_depth = Some(bid_depth);
        self.ask_depth = Some(ask_depth);
        self
    }

    /// Builder: set ADV
    pub fn with_adv(mut self, adv: Decimal) -> Self {
        self.adv = adv;
        self
    }

    /// Builder: set volatility
    pub fn with_volatility(mut self, volatility: Decimal) -> Self {
        self.volatility = volatility;
        self
    }

    /// Builder: set typical spread
    pub fn with_spread(mut self, spread_bps: Decimal) -> Self {
        self.typical_spread_bps = spread_bps;
        self
    }

    /// Calculate mid price
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid, self.best_ask) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
            _ => None,
        }
    }

    /// Calculate current spread in basis points
    pub fn current_spread_bps(&self) -> Option<Decimal> {
        match (self.best_bid, self.best_ask, self.mid_price()) {
            (Some(bid), Some(ask), Some(mid)) if mid > Decimal::ZERO => {
                Some((ask - bid) / mid * Decimal::from(10000))
            }
            _ => None,
        }
    }

    /// Participation rate for a given quantity
    pub fn participation_rate(&self, quantity: Decimal) -> Decimal {
        if self.adv > Decimal::ZERO {
            quantity / self.adv
        } else {
            Decimal::ZERO
        }
    }
}

/// Order specification for TCA calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSpec {
    /// Instrument identifier
    pub instrument_id: String,
    /// Order side (true = buy, false = sell)
    pub is_buy: bool,
    /// Total quantity to execute
    pub quantity: Decimal,
    /// Maximum time horizon (in seconds) for execution
    pub time_horizon_secs: u64,
    /// Risk aversion parameter (higher = more aggressive execution)
    pub risk_aversion: Decimal,
    /// Decision price (price when order was decided)
    pub decision_price: Option<Decimal>,
    /// Timestamp when order was created
    pub timestamp: DateTime<Utc>,
}

impl OrderSpec {
    /// Create new order specification
    pub fn new(
        instrument_id: impl Into<String>,
        is_buy: bool,
        quantity: Decimal,
        time_horizon_secs: u64,
    ) -> Self {
        Self {
            instrument_id: instrument_id.into(),
            is_buy,
            quantity,
            time_horizon_secs,
            risk_aversion: Decimal::ONE,
            decision_price: None,
            timestamp: Utc::now(),
        }
    }

    /// Builder: set risk aversion
    pub fn with_risk_aversion(mut self, lambda: Decimal) -> Self {
        self.risk_aversion = lambda;
        self
    }

    /// Builder: set decision price
    pub fn with_decision_price(mut self, price: Decimal) -> Self {
        self.decision_price = Some(price);
        self
    }

    /// Calculate participation rate
    pub fn participation_rate(&self, adv: Decimal) -> Decimal {
        if adv > Decimal::ZERO {
            self.quantity / adv
        } else {
            Decimal::ZERO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_market_state() {
        let state = MarketState::new("BTC-USD")
            .with_bbo(dec!(50000), dec!(50010))
            .with_adv(dec!(10000))
            .with_volatility(dec!(0.50));

        assert_eq!(state.mid_price(), Some(dec!(50005)));
        assert!(state.current_spread_bps().unwrap() < dec!(3)); // ~2 bps
        assert_eq!(state.participation_rate(dec!(100)), dec!(0.01)); // 1%
    }

    #[test]
    fn test_order_spec() {
        let order = OrderSpec::new("BTC-USD", true, dec!(100), 3600)
            .with_risk_aversion(dec!(0.001))
            .with_decision_price(dec!(50000));

        assert_eq!(order.participation_rate(dec!(10000)), dec!(0.01));
    }
}
