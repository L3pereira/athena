//! Execution Benchmarks
//!
//! Standard benchmarks for measuring execution quality.
//!
//! # Benchmarks
//!
//! | Benchmark | What It Measures |
//! |-----------|------------------|
//! | **Arrival Price** | Slippage from decision time price |
//! | **VWAP** | Performance vs volume-weighted average |
//! | **TWAP** | Performance vs time-weighted average |
//! | **Implementation Shortfall** | Total cost including opportunity cost |
//! | **Close** | Performance vs closing price |

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Type of benchmark
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenchmarkType {
    /// Price at decision time
    ArrivalPrice,
    /// Volume-weighted average price over execution period
    Vwap,
    /// Time-weighted average price over execution period
    Twap,
    /// Full implementation shortfall (decision price + opportunity cost)
    ImplementationShortfall,
    /// Closing price
    Close,
    /// Opening price
    Open,
    /// Mid-price at execution start
    StartMid,
    /// Mid-price at execution end
    EndMid,
}

impl std::fmt::Display for BenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchmarkType::ArrivalPrice => write!(f, "Arrival"),
            BenchmarkType::Vwap => write!(f, "VWAP"),
            BenchmarkType::Twap => write!(f, "TWAP"),
            BenchmarkType::ImplementationShortfall => write!(f, "IS"),
            BenchmarkType::Close => write!(f, "Close"),
            BenchmarkType::Open => write!(f, "Open"),
            BenchmarkType::StartMid => write!(f, "StartMid"),
            BenchmarkType::EndMid => write!(f, "EndMid"),
        }
    }
}

/// A single benchmark value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Benchmark {
    /// Type of benchmark
    pub benchmark_type: BenchmarkType,
    /// Benchmark price
    pub price: Decimal,
    /// When the benchmark was captured
    pub timestamp: DateTime<Utc>,
    /// Volume at benchmark time (if applicable)
    pub volume: Option<Decimal>,
}

impl Benchmark {
    /// Create new benchmark
    pub fn new(benchmark_type: BenchmarkType, price: Decimal) -> Self {
        Self {
            benchmark_type,
            price,
            timestamp: Utc::now(),
            volume: None,
        }
    }

    /// Create with timestamp
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Create with volume
    pub fn with_volume(mut self, volume: Decimal) -> Self {
        self.volume = Some(volume);
        self
    }
}

/// Collection of benchmarks for an execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBenchmarks {
    /// Instrument
    pub instrument_id: String,
    /// Decision timestamp (when order was decided)
    pub decision_time: DateTime<Utc>,
    /// Execution start timestamp
    pub execution_start: DateTime<Utc>,
    /// Execution end timestamp
    pub execution_end: Option<DateTime<Utc>>,
    /// All captured benchmarks
    pub benchmarks: Vec<Benchmark>,
}

impl ExecutionBenchmarks {
    /// Create new benchmark collection
    pub fn new(instrument_id: impl Into<String>, decision_time: DateTime<Utc>) -> Self {
        Self {
            instrument_id: instrument_id.into(),
            decision_time,
            execution_start: decision_time,
            execution_end: None,
            benchmarks: Vec::new(),
        }
    }

    /// Add a benchmark
    pub fn add_benchmark(&mut self, benchmark: Benchmark) {
        self.benchmarks.push(benchmark);
    }

    /// Set arrival price benchmark
    pub fn set_arrival_price(&mut self, price: Decimal) {
        self.benchmarks.push(
            Benchmark::new(BenchmarkType::ArrivalPrice, price).with_timestamp(self.decision_time),
        );
    }

    /// Set VWAP benchmark
    pub fn set_vwap(&mut self, price: Decimal, total_volume: Decimal) {
        self.benchmarks
            .push(Benchmark::new(BenchmarkType::Vwap, price).with_volume(total_volume));
    }

    /// Set TWAP benchmark
    pub fn set_twap(&mut self, price: Decimal) {
        self.benchmarks
            .push(Benchmark::new(BenchmarkType::Twap, price));
    }

    /// Get benchmark by type
    pub fn get(&self, benchmark_type: BenchmarkType) -> Option<&Benchmark> {
        self.benchmarks
            .iter()
            .find(|b| b.benchmark_type == benchmark_type)
    }

    /// Get arrival price
    pub fn arrival_price(&self) -> Option<Decimal> {
        self.get(BenchmarkType::ArrivalPrice).map(|b| b.price)
    }

    /// Get VWAP
    pub fn vwap(&self) -> Option<Decimal> {
        self.get(BenchmarkType::Vwap).map(|b| b.price)
    }

    /// Calculate slippage against a benchmark
    ///
    /// Returns slippage in basis points (positive = underperformance)
    pub fn calculate_slippage(
        &self,
        benchmark_type: BenchmarkType,
        execution_price: Decimal,
        is_buy: bool,
    ) -> Option<Decimal> {
        self.get(benchmark_type).map(|benchmark| {
            let benchmark_price = benchmark.price;
            if benchmark_price.is_zero() {
                return Decimal::ZERO;
            }

            // For buys: positive slippage means we paid more than benchmark
            // For sells: positive slippage means we received less than benchmark
            let raw_slippage = if is_buy {
                (execution_price - benchmark_price) / benchmark_price
            } else {
                (benchmark_price - execution_price) / benchmark_price
            };

            raw_slippage * dec!(10000) // Convert to bps
        })
    }
}

/// Calculator for VWAP and TWAP from trade data
pub struct BenchmarkCalculator;

impl BenchmarkCalculator {
    /// Calculate VWAP from trades
    pub fn calculate_vwap(trades: &[(Decimal, Decimal)]) -> Option<Decimal> {
        // trades: Vec<(price, quantity)>
        if trades.is_empty() {
            return None;
        }

        let mut value_sum = Decimal::ZERO;
        let mut volume_sum = Decimal::ZERO;

        for (price, quantity) in trades {
            value_sum += *price * *quantity;
            volume_sum += *quantity;
        }

        if volume_sum > Decimal::ZERO {
            Some(value_sum / volume_sum)
        } else {
            None
        }
    }

    /// Calculate TWAP from price samples
    pub fn calculate_twap(prices: &[Decimal]) -> Option<Decimal> {
        if prices.is_empty() {
            return None;
        }

        let sum: Decimal = prices.iter().sum();
        Some(sum / Decimal::from(prices.len() as u64))
    }

    /// Calculate time-weighted prices from timestamped data
    pub fn calculate_twap_weighted(samples: &[(DateTime<Utc>, Decimal)]) -> Option<Decimal> {
        if samples.len() < 2 {
            return samples.first().map(|(_, p)| *p);
        }

        let mut weighted_sum = Decimal::ZERO;
        let mut total_duration = Decimal::ZERO;

        for i in 0..samples.len() - 1 {
            let (t1, p1) = &samples[i];
            let (t2, _) = &samples[i + 1];
            let duration = (*t2 - *t1).num_seconds().max(0);
            let duration_dec = Decimal::from(duration);

            weighted_sum += *p1 * duration_dec;
            total_duration += duration_dec;
        }

        // Add last point with zero duration (or could use closing duration)
        if let Some((_, last_price)) = samples.last() {
            // Weight last price equally to average interval
            let avg_interval = total_duration / Decimal::from(samples.len() as u64 - 1);
            weighted_sum += *last_price * avg_interval;
            total_duration += avg_interval;
        }

        if total_duration > Decimal::ZERO {
            Some(weighted_sum / total_duration)
        } else {
            None
        }
    }

    /// Calculate interval VWAP (for a specific time bucket)
    pub fn calculate_interval_vwap(
        trades: &[(DateTime<Utc>, Decimal, Decimal)], // (timestamp, price, quantity)
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Option<Decimal> {
        let filtered: Vec<(Decimal, Decimal)> = trades
            .iter()
            .filter(|(t, _, _)| *t >= start && *t < end)
            .map(|(_, p, q)| (*p, *q))
            .collect();

        Self::calculate_vwap(&filtered)
    }
}

/// Implementation Shortfall calculator
pub struct ImplementationShortfallCalculator;

impl ImplementationShortfallCalculator {
    /// Calculate full implementation shortfall
    ///
    /// IS = Execution Cost + Opportunity Cost + Fees
    ///
    /// Where:
    /// - Execution Cost = actual_price - arrival_price (for buys)
    /// - Opportunity Cost = (end_price - arrival_price) × unfilled_quantity
    pub fn calculate(
        arrival_price: Decimal,
        execution_price: Decimal,
        executed_quantity: Decimal,
        intended_quantity: Decimal,
        end_price: Decimal,
        is_buy: bool,
        fees: Decimal,
    ) -> ImplementationShortfall {
        let unfilled_quantity = intended_quantity - executed_quantity;

        // Execution cost (slippage on filled portion)
        let execution_cost = if is_buy {
            (execution_price - arrival_price) * executed_quantity
        } else {
            (arrival_price - execution_price) * executed_quantity
        };

        // Opportunity cost (market moved away on unfilled portion)
        let opportunity_cost = if is_buy {
            // If buying and price went up, we lost opportunity
            ((end_price - arrival_price).max(Decimal::ZERO)) * unfilled_quantity
        } else {
            // If selling and price went down, we lost opportunity
            ((arrival_price - end_price).max(Decimal::ZERO)) * unfilled_quantity
        };

        let total_cost = execution_cost + opportunity_cost + fees;

        // Convert to bps relative to notional
        let notional = arrival_price * intended_quantity;
        let total_bps = if notional > Decimal::ZERO {
            total_cost / notional * dec!(10000)
        } else {
            Decimal::ZERO
        };

        ImplementationShortfall {
            arrival_price,
            execution_price,
            end_price,
            executed_quantity,
            intended_quantity,
            execution_cost,
            opportunity_cost,
            fees,
            total_cost,
            total_bps,
            fill_rate: if intended_quantity > Decimal::ZERO {
                executed_quantity / intended_quantity
            } else {
                Decimal::ONE
            },
        }
    }
}

/// Implementation Shortfall breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationShortfall {
    /// Price at decision time
    pub arrival_price: Decimal,
    /// Average execution price
    pub execution_price: Decimal,
    /// Price at end of execution window
    pub end_price: Decimal,
    /// Quantity actually executed
    pub executed_quantity: Decimal,
    /// Quantity intended to execute
    pub intended_quantity: Decimal,
    /// Cost from execution slippage
    pub execution_cost: Decimal,
    /// Cost from unfilled quantity (market moved away)
    pub opportunity_cost: Decimal,
    /// Trading fees
    pub fees: Decimal,
    /// Total cost
    pub total_cost: Decimal,
    /// Total cost in basis points
    pub total_bps: Decimal,
    /// Fill rate (executed / intended)
    pub fill_rate: Decimal,
}

impl ImplementationShortfall {
    /// Get execution cost in bps
    pub fn execution_cost_bps(&self) -> Decimal {
        let notional = self.arrival_price * self.intended_quantity;
        if notional > Decimal::ZERO {
            self.execution_cost / notional * dec!(10000)
        } else {
            Decimal::ZERO
        }
    }

    /// Get opportunity cost in bps
    pub fn opportunity_cost_bps(&self) -> Decimal {
        let notional = self.arrival_price * self.intended_quantity;
        if notional > Decimal::ZERO {
            self.opportunity_cost / notional * dec!(10000)
        } else {
            Decimal::ZERO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_creation() {
        let mut benchmarks = ExecutionBenchmarks::new("BTC-USD", Utc::now());
        benchmarks.set_arrival_price(dec!(50000));
        benchmarks.set_vwap(dec!(50025), dec!(1000));

        assert_eq!(benchmarks.arrival_price(), Some(dec!(50000)));
        assert_eq!(benchmarks.vwap(), Some(dec!(50025)));
    }

    #[test]
    fn test_slippage_calculation() {
        let mut benchmarks = ExecutionBenchmarks::new("BTC-USD", Utc::now());
        benchmarks.set_arrival_price(dec!(50000));

        // Bought at 50050 when arrival was 50000 = 10 bps slippage
        let slippage = benchmarks
            .calculate_slippage(BenchmarkType::ArrivalPrice, dec!(50050), true)
            .unwrap();
        assert!((slippage - dec!(10)).abs() < dec!(0.1));

        // Sold at 49950 when arrival was 50000 = 10 bps slippage
        let slippage = benchmarks
            .calculate_slippage(BenchmarkType::ArrivalPrice, dec!(49950), false)
            .unwrap();
        assert!((slippage - dec!(10)).abs() < dec!(0.1));
    }

    #[test]
    fn test_vwap_calculation() {
        let trades = vec![
            (dec!(100), dec!(10)), // $100 × 10 = $1000
            (dec!(102), dec!(20)), // $102 × 20 = $2040
            (dec!(101), dec!(10)), // $101 × 10 = $1010
        ];

        let vwap = BenchmarkCalculator::calculate_vwap(&trades).unwrap();
        // Total value: $4050, Total volume: 40
        // VWAP = 4050/40 = 101.25
        assert!((vwap - dec!(101.25)).abs() < dec!(0.01));
    }

    #[test]
    fn test_twap_calculation() {
        let prices = vec![dec!(100), dec!(102), dec!(101), dec!(103)];
        let twap = BenchmarkCalculator::calculate_twap(&prices).unwrap();
        // (100 + 102 + 101 + 103) / 4 = 101.5
        assert!((twap - dec!(101.5)).abs() < dec!(0.01));
    }

    #[test]
    fn test_implementation_shortfall() {
        let is = ImplementationShortfallCalculator::calculate(
            dec!(100), // arrival
            dec!(101), // execution (paid 1% more)
            dec!(80),  // executed 80 of 100
            dec!(100), // intended
            dec!(103), // end price (market moved against us)
            true,      // buy
            dec!(5),   // fees
        );

        // Execution cost: (101 - 100) × 80 = 80
        assert_eq!(is.execution_cost, dec!(80));

        // Opportunity cost: (103 - 100) × 20 = 60
        assert_eq!(is.opportunity_cost, dec!(60));

        // Total: 80 + 60 + 5 = 145
        assert_eq!(is.total_cost, dec!(145));

        // Fill rate: 80%
        assert_eq!(is.fill_rate, dec!(0.8));

        println!("IS: {} bps", is.total_bps);
    }

    #[test]
    fn test_implementation_shortfall_sell() {
        let is = ImplementationShortfallCalculator::calculate(
            dec!(100), // arrival
            dec!(99),  // execution (received 1% less)
            dec!(100), // executed all
            dec!(100), // intended
            dec!(100), // end price (no move)
            false,     // sell
            dec!(5),   // fees
        );

        // Execution cost: (100 - 99) × 100 = 100
        assert_eq!(is.execution_cost, dec!(100));

        // No opportunity cost (fully filled)
        assert_eq!(is.opportunity_cost, Decimal::ZERO);

        // Total: 100 + 0 + 5 = 105
        assert_eq!(is.total_cost, dec!(105));
    }
}
