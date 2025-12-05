//! Post-Trade TCA Measurement
//!
//! Measures execution quality after trades are complete.
//!
//! # Metrics
//!
//! - **Arrival Slippage**: Cost vs decision price
//! - **VWAP Performance**: How well we beat/missed VWAP
//! - **Implementation Shortfall**: Full cost attribution
//! - **Realized vs Estimated**: Compare actual to pre-trade estimate

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::benchmark::{
    BenchmarkCalculator, BenchmarkType, ExecutionBenchmarks, ImplementationShortfallCalculator,
};
use super::estimator::TcaEstimate;

/// Record of a single execution (fill)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    /// Execution ID
    pub execution_id: String,
    /// Order ID this execution belongs to
    pub order_id: String,
    /// Instrument
    pub instrument_id: String,
    /// Is buy
    pub is_buy: bool,
    /// Executed quantity
    pub quantity: Decimal,
    /// Execution price
    pub price: Decimal,
    /// Fees paid
    pub fees: Decimal,
    /// Execution timestamp
    pub timestamp: DateTime<Utc>,
    /// Venue/exchange
    pub venue: Option<String>,
}

impl ExecutionRecord {
    /// Create new execution record
    pub fn new(
        execution_id: impl Into<String>,
        order_id: impl Into<String>,
        instrument_id: impl Into<String>,
        is_buy: bool,
        quantity: Decimal,
        price: Decimal,
    ) -> Self {
        Self {
            execution_id: execution_id.into(),
            order_id: order_id.into(),
            instrument_id: instrument_id.into(),
            is_buy,
            quantity,
            price,
            fees: Decimal::ZERO,
            timestamp: Utc::now(),
            venue: None,
        }
    }

    /// Builder: set fees
    pub fn with_fees(mut self, fees: Decimal) -> Self {
        self.fees = fees;
        self
    }

    /// Builder: set timestamp
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Builder: set venue
    pub fn with_venue(mut self, venue: impl Into<String>) -> Self {
        self.venue = Some(venue.into());
        self
    }

    /// Calculate notional value
    pub fn notional(&self) -> Decimal {
        self.quantity * self.price
    }
}

/// Complete TCA metrics for an order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcaMetrics {
    /// Order ID
    pub order_id: String,
    /// Instrument
    pub instrument_id: String,
    /// Is buy order
    pub is_buy: bool,

    // Execution summary
    /// Total quantity executed
    pub executed_quantity: Decimal,
    /// Intended quantity
    pub intended_quantity: Decimal,
    /// Fill rate (executed / intended)
    pub fill_rate: Decimal,
    /// Average execution price
    pub avg_execution_price: Decimal,
    /// Total fees
    pub total_fees: Decimal,
    /// Number of fills
    pub num_fills: usize,

    // Timing
    /// Decision timestamp
    pub decision_time: DateTime<Utc>,
    /// First fill timestamp
    pub first_fill_time: Option<DateTime<Utc>>,
    /// Last fill timestamp
    pub last_fill_time: Option<DateTime<Utc>>,
    /// Total execution duration (seconds)
    pub execution_duration_secs: u64,

    // Slippage metrics (all in bps)
    /// Slippage vs arrival price
    pub arrival_slippage_bps: Option<Decimal>,
    /// Slippage vs VWAP
    pub vwap_slippage_bps: Option<Decimal>,
    /// Slippage vs TWAP
    pub twap_slippage_bps: Option<Decimal>,
    /// Total implementation shortfall
    pub implementation_shortfall_bps: Option<Decimal>,

    // Comparison to estimate
    /// Pre-trade cost estimate (if available)
    pub estimated_cost_bps: Option<Decimal>,
    /// Actual realized cost
    pub realized_cost_bps: Decimal,
    /// Estimate error (realized - estimated)
    pub estimate_error_bps: Option<Decimal>,

    // Venue analysis
    /// Execution breakdown by venue
    pub venue_breakdown: HashMap<String, VenueStats>,
}

impl TcaMetrics {
    /// Get overall performance grade
    pub fn performance_grade(&self) -> PerformanceGrade {
        // Based on arrival slippage
        if let Some(slippage) = self.arrival_slippage_bps {
            if slippage < dec!(-5) {
                PerformanceGrade::Excellent // Beat benchmark by 5+ bps
            } else if slippage < dec!(5) {
                PerformanceGrade::Good // Within 5 bps
            } else if slippage < dec!(15) {
                PerformanceGrade::Fair // 5-15 bps slippage
            } else if slippage < dec!(30) {
                PerformanceGrade::Poor // 15-30 bps slippage
            } else {
                PerformanceGrade::VeryPoor // >30 bps slippage
            }
        } else {
            PerformanceGrade::Unknown
        }
    }

    /// Generate summary report
    pub fn summary(&self) -> String {
        format!(
            "TCA Summary for {} ({})\n\
             Fill Rate: {:.1}% ({} fills)\n\
             Avg Price: {:.4}\n\
             Arrival Slippage: {} bps\n\
             VWAP Slippage: {} bps\n\
             IS: {} bps\n\
             Grade: {:?}",
            self.order_id,
            self.instrument_id,
            self.fill_rate * dec!(100),
            self.num_fills,
            self.avg_execution_price,
            self.arrival_slippage_bps
                .map(|s| format!("{:.1}", s))
                .unwrap_or_else(|| "N/A".to_string()),
            self.vwap_slippage_bps
                .map(|s| format!("{:.1}", s))
                .unwrap_or_else(|| "N/A".to_string()),
            self.implementation_shortfall_bps
                .map(|s| format!("{:.1}", s))
                .unwrap_or_else(|| "N/A".to_string()),
            self.performance_grade(),
        )
    }
}

/// Performance grade based on slippage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceGrade {
    Excellent,
    Good,
    Fair,
    Poor,
    VeryPoor,
    Unknown,
}

/// Statistics for a single venue
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VenueStats {
    /// Number of fills
    pub fill_count: usize,
    /// Total quantity
    pub total_quantity: Decimal,
    /// Volume-weighted average price
    pub vwap: Decimal,
    /// Total fees
    pub total_fees: Decimal,
    /// Average fill size
    pub avg_fill_size: Decimal,
}

/// TCA measurement engine
pub struct TcaMeasurement;

impl TcaMeasurement {
    /// Measure TCA metrics for an order
    pub fn measure(
        order_id: &str,
        instrument_id: &str,
        intended_quantity: Decimal,
        is_buy: bool,
        executions: &[ExecutionRecord],
        benchmarks: &ExecutionBenchmarks,
        pre_trade_estimate: Option<&TcaEstimate>,
    ) -> TcaMetrics {
        // Calculate execution statistics
        let executed_quantity: Decimal = executions.iter().map(|e| e.quantity).sum();
        let total_fees: Decimal = executions.iter().map(|e| e.fees).sum();
        let num_fills = executions.len();

        let fill_rate = if intended_quantity > Decimal::ZERO {
            executed_quantity / intended_quantity
        } else {
            Decimal::ONE
        };

        // Calculate average execution price (VWAP of our fills)
        let avg_execution_price = if !executions.is_empty() {
            let trades: Vec<(Decimal, Decimal)> =
                executions.iter().map(|e| (e.price, e.quantity)).collect();
            BenchmarkCalculator::calculate_vwap(&trades).unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };

        // Timing
        let decision_time = benchmarks.decision_time;
        let first_fill_time = executions.iter().map(|e| e.timestamp).min();
        let last_fill_time = executions.iter().map(|e| e.timestamp).max();
        let execution_duration_secs = match (first_fill_time, last_fill_time) {
            (Some(first), Some(last)) => (last - first).num_seconds().max(0) as u64,
            _ => 0,
        };

        // Calculate slippage vs benchmarks
        let arrival_slippage_bps =
            benchmarks.calculate_slippage(BenchmarkType::ArrivalPrice, avg_execution_price, is_buy);

        let vwap_slippage_bps =
            benchmarks.calculate_slippage(BenchmarkType::Vwap, avg_execution_price, is_buy);

        let twap_slippage_bps =
            benchmarks.calculate_slippage(BenchmarkType::Twap, avg_execution_price, is_buy);

        // Implementation Shortfall
        let implementation_shortfall_bps = Self::calculate_is(
            benchmarks,
            avg_execution_price,
            executed_quantity,
            intended_quantity,
            is_buy,
            total_fees,
        );

        // Realized cost (arrival slippage + fees in bps)
        let realized_cost_bps = arrival_slippage_bps.unwrap_or(Decimal::ZERO)
            + Self::fees_to_bps(total_fees, avg_execution_price, executed_quantity);

        // Compare to estimate
        let estimated_cost_bps = pre_trade_estimate.map(|e| e.total_cost_bps);
        let estimate_error_bps = estimated_cost_bps.map(|est| realized_cost_bps - est);

        // Venue breakdown
        let venue_breakdown = Self::calculate_venue_breakdown(executions);

        TcaMetrics {
            order_id: order_id.to_string(),
            instrument_id: instrument_id.to_string(),
            is_buy,
            executed_quantity,
            intended_quantity,
            fill_rate,
            avg_execution_price,
            total_fees,
            num_fills,
            decision_time,
            first_fill_time,
            last_fill_time,
            execution_duration_secs,
            arrival_slippage_bps,
            vwap_slippage_bps,
            twap_slippage_bps,
            implementation_shortfall_bps,
            estimated_cost_bps,
            realized_cost_bps,
            estimate_error_bps,
            venue_breakdown,
        }
    }

    /// Calculate Implementation Shortfall
    fn calculate_is(
        benchmarks: &ExecutionBenchmarks,
        execution_price: Decimal,
        executed_quantity: Decimal,
        intended_quantity: Decimal,
        is_buy: bool,
        fees: Decimal,
    ) -> Option<Decimal> {
        let arrival_price = benchmarks.arrival_price()?;
        let end_price = benchmarks
            .get(BenchmarkType::EndMid)
            .map(|b| b.price)
            .unwrap_or(execution_price);

        let is = ImplementationShortfallCalculator::calculate(
            arrival_price,
            execution_price,
            executed_quantity,
            intended_quantity,
            end_price,
            is_buy,
            fees,
        );

        Some(is.total_bps)
    }

    /// Convert fees to basis points
    fn fees_to_bps(fees: Decimal, price: Decimal, quantity: Decimal) -> Decimal {
        let notional = price * quantity;
        if notional > Decimal::ZERO {
            fees / notional * dec!(10000)
        } else {
            Decimal::ZERO
        }
    }

    /// Calculate venue breakdown
    fn calculate_venue_breakdown(executions: &[ExecutionRecord]) -> HashMap<String, VenueStats> {
        let mut breakdown: HashMap<String, VenueStats> = HashMap::new();

        for exec in executions {
            let venue = exec.venue.clone().unwrap_or_else(|| "Unknown".to_string());
            let stats = breakdown.entry(venue).or_default();

            stats.fill_count += 1;
            stats.total_quantity += exec.quantity;
            stats.total_fees += exec.fees;
        }

        // Calculate VWAP and avg fill size for each venue
        for (venue, stats) in breakdown.iter_mut() {
            let venue_executions: Vec<_> = executions
                .iter()
                .filter(|e| {
                    e.venue.as_deref() == Some(venue) || (e.venue.is_none() && venue == "Unknown")
                })
                .collect();

            if !venue_executions.is_empty() {
                let trades: Vec<(Decimal, Decimal)> = venue_executions
                    .iter()
                    .map(|e| (e.price, e.quantity))
                    .collect();
                stats.vwap = BenchmarkCalculator::calculate_vwap(&trades).unwrap_or(Decimal::ZERO);
                stats.avg_fill_size = stats.total_quantity / Decimal::from(stats.fill_count as u64);
            }
        }

        breakdown
    }

    /// Aggregate metrics across multiple orders
    pub fn aggregate(metrics: &[TcaMetrics]) -> AggregatedTcaMetrics {
        if metrics.is_empty() {
            return AggregatedTcaMetrics::default();
        }

        let total_orders = metrics.len();
        let total_notional: Decimal = metrics
            .iter()
            .map(|m| m.avg_execution_price * m.executed_quantity)
            .sum();

        // Volume-weighted slippages
        let mut arrival_weighted = Decimal::ZERO;
        let mut vwap_weighted = Decimal::ZERO;
        let mut is_weighted = Decimal::ZERO;
        let mut total_weight = Decimal::ZERO;

        for m in metrics {
            let weight = m.executed_quantity;
            if weight > Decimal::ZERO {
                if let Some(slip) = m.arrival_slippage_bps {
                    arrival_weighted += slip * weight;
                }
                if let Some(slip) = m.vwap_slippage_bps {
                    vwap_weighted += slip * weight;
                }
                if let Some(is) = m.implementation_shortfall_bps {
                    is_weighted += is * weight;
                }
                total_weight += weight;
            }
        }

        let avg_arrival_slippage = if total_weight > Decimal::ZERO {
            Some(arrival_weighted / total_weight)
        } else {
            None
        };

        let avg_vwap_slippage = if total_weight > Decimal::ZERO {
            Some(vwap_weighted / total_weight)
        } else {
            None
        };

        let avg_is = if total_weight > Decimal::ZERO {
            Some(is_weighted / total_weight)
        } else {
            None
        };

        // Fill rate
        let total_executed: Decimal = metrics.iter().map(|m| m.executed_quantity).sum();
        let total_intended: Decimal = metrics.iter().map(|m| m.intended_quantity).sum();
        let avg_fill_rate = if total_intended > Decimal::ZERO {
            total_executed / total_intended
        } else {
            Decimal::ONE
        };

        // Estimate accuracy
        let estimate_errors: Vec<Decimal> = metrics
            .iter()
            .filter_map(|m| m.estimate_error_bps)
            .collect();

        let avg_estimate_error = if !estimate_errors.is_empty() {
            Some(
                estimate_errors.iter().sum::<Decimal>()
                    / Decimal::from(estimate_errors.len() as u64),
            )
        } else {
            None
        };

        AggregatedTcaMetrics {
            total_orders,
            total_notional,
            avg_fill_rate,
            avg_arrival_slippage_bps: avg_arrival_slippage,
            avg_vwap_slippage_bps: avg_vwap_slippage,
            avg_implementation_shortfall_bps: avg_is,
            avg_estimate_error_bps: avg_estimate_error,
        }
    }
}

/// Aggregated TCA metrics across multiple orders
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregatedTcaMetrics {
    /// Total number of orders
    pub total_orders: usize,
    /// Total notional value traded
    pub total_notional: Decimal,
    /// Average fill rate
    pub avg_fill_rate: Decimal,
    /// Volume-weighted average arrival slippage
    pub avg_arrival_slippage_bps: Option<Decimal>,
    /// Volume-weighted average VWAP slippage
    pub avg_vwap_slippage_bps: Option<Decimal>,
    /// Volume-weighted average implementation shortfall
    pub avg_implementation_shortfall_bps: Option<Decimal>,
    /// Average estimation error
    pub avg_estimate_error_bps: Option<Decimal>,
}

impl AggregatedTcaMetrics {
    /// Generate summary report
    pub fn summary(&self) -> String {
        format!(
            "Aggregated TCA ({} orders, ${:.0} notional)\n\
             Avg Fill Rate: {:.1}%\n\
             Avg Arrival Slippage: {} bps\n\
             Avg VWAP Slippage: {} bps\n\
             Avg IS: {} bps\n\
             Avg Estimate Error: {} bps",
            self.total_orders,
            self.total_notional,
            self.avg_fill_rate * dec!(100),
            self.avg_arrival_slippage_bps
                .map(|s| format!("{:.1}", s))
                .unwrap_or_else(|| "N/A".to_string()),
            self.avg_vwap_slippage_bps
                .map(|s| format!("{:.1}", s))
                .unwrap_or_else(|| "N/A".to_string()),
            self.avg_implementation_shortfall_bps
                .map(|s| format!("{:.1}", s))
                .unwrap_or_else(|| "N/A".to_string()),
            self.avg_estimate_error_bps
                .map(|s| format!("{:+.1}", s))
                .unwrap_or_else(|| "N/A".to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::benchmark::Benchmark;
    use super::*;
    use chrono::Duration;

    fn make_executions() -> Vec<ExecutionRecord> {
        let base_time = Utc::now();
        vec![
            ExecutionRecord::new("exec-1", "order-1", "BTC-USD", true, dec!(10), dec!(50010))
                .with_fees(dec!(5))
                .with_timestamp(base_time)
                .with_venue("Binance"),
            ExecutionRecord::new("exec-2", "order-1", "BTC-USD", true, dec!(20), dec!(50020))
                .with_fees(dec!(10))
                .with_timestamp(base_time + Duration::seconds(30))
                .with_venue("Binance"),
            ExecutionRecord::new("exec-3", "order-1", "BTC-USD", true, dec!(15), dec!(50015))
                .with_fees(dec!(7))
                .with_timestamp(base_time + Duration::seconds(60))
                .with_venue("Coinbase"),
        ]
    }

    fn make_benchmarks() -> ExecutionBenchmarks {
        let mut benchmarks = ExecutionBenchmarks::new("BTC-USD", Utc::now());
        benchmarks.set_arrival_price(dec!(50000));
        benchmarks.set_vwap(dec!(50012), dec!(10000));
        benchmarks.set_twap(dec!(50010));
        benchmarks.add_benchmark(Benchmark::new(BenchmarkType::EndMid, dec!(50030)));
        benchmarks
    }

    #[test]
    fn test_tca_measurement() {
        let executions = make_executions();
        let benchmarks = make_benchmarks();

        let metrics = TcaMeasurement::measure(
            "order-1",
            "BTC-USD",
            dec!(50), // intended
            true,     // buy
            &executions,
            &benchmarks,
            None,
        );

        assert_eq!(metrics.executed_quantity, dec!(45));
        assert_eq!(metrics.fill_rate, dec!(0.9)); // 45/50
        assert_eq!(metrics.num_fills, 3);
        assert_eq!(metrics.total_fees, dec!(22));

        // Check average price is reasonable
        // VWAP = (50010*10 + 50020*20 + 50015*15) / 45 ≈ 50016.11
        assert!(metrics.avg_execution_price > dec!(50015));
        assert!(metrics.avg_execution_price < dec!(50020));

        // Should have positive slippage (paid more than arrival)
        assert!(metrics.arrival_slippage_bps.unwrap() > Decimal::ZERO);

        println!("{}", metrics.summary());
    }

    #[test]
    fn test_venue_breakdown() {
        let executions = make_executions();
        let benchmarks = make_benchmarks();

        let metrics = TcaMeasurement::measure(
            "order-1",
            "BTC-USD",
            dec!(50),
            true,
            &executions,
            &benchmarks,
            None,
        );

        assert!(metrics.venue_breakdown.contains_key("Binance"));
        assert!(metrics.venue_breakdown.contains_key("Coinbase"));

        let binance = &metrics.venue_breakdown["Binance"];
        assert_eq!(binance.fill_count, 2);
        assert_eq!(binance.total_quantity, dec!(30));

        let coinbase = &metrics.venue_breakdown["Coinbase"];
        assert_eq!(coinbase.fill_count, 1);
        assert_eq!(coinbase.total_quantity, dec!(15));
    }

    #[test]
    fn test_performance_grades() {
        let mut metrics = TcaMeasurement::measure(
            "order-1",
            "BTC-USD",
            dec!(45),
            true,
            &make_executions(),
            &make_benchmarks(),
            None,
        );

        // Artificially set slippage to test grades
        metrics.arrival_slippage_bps = Some(dec!(-10));
        assert_eq!(metrics.performance_grade(), PerformanceGrade::Excellent);

        metrics.arrival_slippage_bps = Some(dec!(3));
        assert_eq!(metrics.performance_grade(), PerformanceGrade::Good);

        metrics.arrival_slippage_bps = Some(dec!(12));
        assert_eq!(metrics.performance_grade(), PerformanceGrade::Fair);

        metrics.arrival_slippage_bps = Some(dec!(25));
        assert_eq!(metrics.performance_grade(), PerformanceGrade::Poor);

        metrics.arrival_slippage_bps = Some(dec!(50));
        assert_eq!(metrics.performance_grade(), PerformanceGrade::VeryPoor);
    }

    #[test]
    fn test_aggregate_metrics() {
        let executions = make_executions();
        let benchmarks = make_benchmarks();

        let m1 = TcaMeasurement::measure(
            "order-1",
            "BTC-USD",
            dec!(45),
            true,
            &executions,
            &benchmarks,
            None,
        );

        let m2 = TcaMeasurement::measure(
            "order-2",
            "BTC-USD",
            dec!(100),
            true,
            &executions,
            &benchmarks,
            None,
        );

        let aggregate = TcaMeasurement::aggregate(&[m1, m2]);

        assert_eq!(aggregate.total_orders, 2);
        assert!(aggregate.total_notional > Decimal::ZERO);
        assert!(aggregate.avg_arrival_slippage_bps.is_some());

        println!("{}", aggregate.summary());
    }
}
