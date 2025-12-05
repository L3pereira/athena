# Transaction Cost Analysis (TCA)

A production-grade Transaction Cost Analysis system for execution cost estimation, optimal scheduling, and post-trade measurement.

## Overview

The TCA module implements industry-standard models from academic literature:

- **Kyle (1985)**: Linear price impact from informed trading
- **Almgren-Chriss (2000)**: Optimal execution with temporary + permanent impact
- **Bouchaud et al.**: Empirical square-root law for market impact

## Module Structure

```
crates/order-manager/src/tca/
├── mod.rs          # Module root, common types (MarketState, OrderSpec)
├── models.rs       # Impact models (Kyle, Almgren-Chriss, Square-Root)
├── estimator.rs    # Pre-trade cost estimation
├── scheduler.rs    # Optimal execution algorithms (TWAP, VWAP, IS)
├── measurement.rs  # Post-trade TCA measurement
├── benchmark.rs    # Execution benchmarks (Arrival, VWAP, IS)
└── calibration.rs  # Parameter calibration from historical data
```

## Impact Models

### Kyle (1985) - Linear Impact

The classic model where impact is proportional to order size:

```
Impact = λ × (Q / ADV)
```

Where:
- λ (Kyle's lambda) = price sensitivity to order flow
- Q = order quantity
- ADV = average daily volume

**Use case**: Simple baseline, informed trading analysis

```rust
use athena_order_manager::tca::{KyleModel, KyleParams};

let model = KyleModel::new(KyleParams {
    lambda: dec!(10),      // 10 bps per 1% of ADV
    all_permanent: true,   // Classic Kyle: all impact is permanent
    ..Default::default()
});
```

### Almgren-Chriss (2000) - Temporary + Permanent Impact

Separates impact into temporary (transient) and permanent components:

```
Temporary Impact = η × (Q/T)^α     # Rate-dependent, decays
Permanent Impact = γ × Q           # Permanent information leakage
Total Impact = Temporary + Permanent
```

Where:
- η (eta) = temporary impact coefficient
- γ (gamma) = permanent impact coefficient
- α = impact exponent (typically 0.5-1.0)

**Use case**: Optimal execution trajectory calculation

```rust
use athena_order_manager::tca::{AlmgrenChrissModel, AlmgrenChrissParams};

let model = AlmgrenChrissModel::new(AlmgrenChrissParams {
    gamma: dec!(0.1),          // Permanent impact
    eta: dec!(0.05),           // Temporary impact
    eta_exponent: dec!(0.6),   // Slightly concave
    ..Default::default()
});

// Generate optimal trading trajectory
let trajectory = model.optimal_trajectory(
    dec!(1000),     // Total quantity
    10,             // 10 periods
    dec!(0.001),    // Risk aversion λ
    &market_state,
);
```

### Square-Root (Bouchaud et al.) - Empirical Law

Empirically validated concave relationship:

```
Impact = Y × σ × √(Q/V)
```

Where:
- Y ≈ 0.3 (calibrated constant)
- σ = daily volatility
- Q = order quantity
- V = average daily volume

**Use case**: Pre-trade cost estimation (most accurate for real markets)

```rust
use athena_order_manager::tca::{SquareRootModel, SquareRootParams};

let model = SquareRootModel::new(SquareRootParams {
    y_coefficient: dec!(0.3),       // ~1/π empirically
    temporary_fraction: dec!(0.7),  // 70% temporary
    decay_half_life_secs: 300,      // 5 minute decay
});
```

## Pre-Trade Cost Estimation

Estimates execution costs before trading:

```
Total Cost = Spread Cost + Market Impact + Timing Risk + Fees
           = (spread/2) + f(Q,V,σ) + λσ²T + fees
```

```rust
use athena_order_manager::tca::{TcaEstimator, TcaEstimatorConfig, MarketState, OrderSpec};

let estimator = TcaEstimator::new(TcaEstimatorConfig {
    fee_bps: dec!(5),
    include_timing_risk: true,
    timing_risk_confidence: dec!(0.95),
    ..Default::default()
});

let market = MarketState::new("BTC-USD")
    .with_bbo(dec!(50000), dec!(50010))
    .with_adv(dec!(10000))
    .with_volatility(dec!(0.50));

let order = OrderSpec::new("BTC-USD", true, dec!(100), 3600);

let estimate = estimator.estimate(&order, &market);
println!("{}", estimate.cost_breakdown());
// Output: Spread: 1.0 bps | Impact: 47.4 bps | Timing: 28.9 bps | Fees: 5.0 bps | Total: 82.3 bps

// Check if alpha justifies the cost
let (should_trade, _) = estimator.is_alpha_sufficient(
    &order,
    &market,
    dec!(150),  // Expected alpha in bps
    dec!(1.5),  // Cost buffer multiplier
);
```

## Optimal Execution Schedulers

### Available Algorithms

| Algorithm | Objective | Best For |
|-----------|-----------|----------|
| **TWAP** | Minimize timing risk | Low urgency, uniform execution |
| **VWAP** | Match volume profile | Reduce market impact |
| **Implementation Shortfall** | min E[Cost] + λ×Var[Cost] | Optimize urgency vs cost |
| **POV** | Trade at fixed % of volume | Participation rate limits |
| **Adaptive** | Adjust to conditions | Dynamic execution |

### TWAP (Time-Weighted Average Price)

Trades evenly over time:

```rust
use athena_order_manager::tca::{ExecutionScheduler, SchedulerType};

let scheduler = ExecutionScheduler::new(SchedulerType::Twap);
let schedule = scheduler.generate_schedule(
    "BTC-USD",
    true,           // is_buy
    dec!(100),      // quantity
    3600,           // duration_secs
    10,             // num_slices
    &market_state,
);

for slice in &schedule.slices {
    println!("Slice {}: {} @ {:?}", slice.index, slice.quantity, slice.start_time);
}
```

### Implementation Shortfall (Almgren-Chriss Optimal)

Minimizes total cost including timing risk:

```
min E[Cost] + λ × Var[Cost]
```

Higher λ = more aggressive (front-loaded), Lower λ = more patient (spread out)

```rust
let scheduler = ExecutionScheduler::new(SchedulerType::ImplementationShortfall {
    risk_aversion: dec!(0.001),  // Higher = more aggressive
});
```

### VWAP (Volume-Weighted Average Price)

Follows expected volume profile:

```rust
use athena_order_manager::tca::VolumeProfile;

let scheduler = ExecutionScheduler::new(SchedulerType::Vwap {
    volume_profile: VolumeProfile::crypto_24h(),  // Flat for crypto
});
```

### Adaptive

Dynamically adjusts based on spread and volatility:

```rust
let scheduler = ExecutionScheduler::new(SchedulerType::Adaptive {
    base_strategy: Box::new(SchedulerType::Twap),
    aggression_factor: dec!(1.2),  // 1.0 = neutral, >1 = frontload
});
```

## Post-Trade Measurement

### Benchmarks

| Benchmark | What It Measures |
|-----------|------------------|
| **Arrival Price** | Slippage from decision time |
| **VWAP** | Did you beat average price? |
| **TWAP** | Performance vs time-average |
| **Implementation Shortfall** | Full cost attribution |

```rust
use athena_order_manager::tca::{
    ExecutionBenchmarks, BenchmarkType, TcaMeasurement, ExecutionRecord,
};

// Set up benchmarks
let mut benchmarks = ExecutionBenchmarks::new("BTC-USD", decision_time);
benchmarks.set_arrival_price(dec!(50000));
benchmarks.set_vwap(dec!(50025), dec!(10000));

// Record executions
let executions = vec![
    ExecutionRecord::new("exec-1", "order-1", "BTC-USD", true, dec!(50), dec!(50010))
        .with_fees(dec!(5))
        .with_venue("Binance"),
    ExecutionRecord::new("exec-2", "order-1", "BTC-USD", true, dec!(50), dec!(50020))
        .with_fees(dec!(5))
        .with_venue("Coinbase"),
];

// Measure TCA
let metrics = TcaMeasurement::measure(
    "order-1",
    "BTC-USD",
    dec!(100),      // intended quantity
    true,           // is_buy
    &executions,
    &benchmarks,
    None,           // pre-trade estimate for comparison
);

println!("{}", metrics.summary());
// Output:
// TCA Summary for order-1 (BTC-USD)
// Fill Rate: 100.0% (2 fills)
// Avg Price: 50015.0000
// Arrival Slippage: 3.0 bps
// VWAP Slippage: -2.0 bps
// IS: 3.5 bps
// Grade: Good
```

### Implementation Shortfall Breakdown

Full cost attribution:

```rust
use athena_order_manager::tca::ImplementationShortfallCalculator;

let is = ImplementationShortfallCalculator::calculate(
    dec!(100),      // arrival_price
    dec!(101),      // execution_price
    dec!(80),       // executed_quantity
    dec!(100),      // intended_quantity
    dec!(103),      // end_price (market moved)
    true,           // is_buy
    dec!(5),        // fees
);

// Breakdown:
// - Execution cost: (101 - 100) × 80 = 80
// - Opportunity cost: (103 - 100) × 20 = 60 (unfilled @ higher price)
// - Total: 80 + 60 + 5 = 145
```

## Calibration Framework

Fit impact parameters from historical execution data:

```rust
use athena_order_manager::tca::{
    ImpactCalibrator, CalibrationConfig, ExecutionDataPoint,
};

// Historical execution data
let data: Vec<ExecutionDataPoint> = historical_fills
    .into_iter()
    .map(|fill| ExecutionDataPoint {
        instrument_id: fill.instrument_id,
        quantity: fill.quantity,
        adv: get_adv(&fill.instrument_id),
        volatility: get_volatility(&fill.instrument_id),
        spread_bps: fill.spread_at_execution,
        realized_impact_bps: calculate_impact(&fill),
        duration_secs: fill.duration,
        is_buy: fill.is_buy,
    })
    .collect();

// Calibrate
let calibrator = ImpactCalibrator::new(CalibrationConfig {
    min_data_points: 30,
    robust_regression: true,    // Downweight outliers
    outlier_threshold: dec!(3), // 3 standard deviations
    ..Default::default()
});

let result = calibrator.calibrate_square_root(&data);

println!("{}", result.report());
// Output:
// Calibration Results (SquareRoot)
// Parameters: {"y_coefficient": 2847.3, "temporary_fraction": 0.7}
// R²: 0.673
// RMSE: 12.4 bps
// MAE: 9.8 bps
// Samples: 1247
// Quality: Good

// Use calibrated model
let params = result.to_square_root_params().unwrap();
let calibrated_model = SquareRootModel::new(params);
```

## Integration with ExecutionPlanner

The TCA module integrates with the existing `ExecutionPlanner`:

```rust
use athena_order_manager::{
    ExecutionPlanner, ExecutionConfig, PortfolioTarget,
    tca::{TcaEstimator, ExecutionScheduler, SchedulerType},
};

// Pre-trade: Check if trade is worth the cost
let estimator = TcaEstimator::default();
let estimate = estimator.estimate(&order_spec, &market_state);

if target.alpha.unwrap_or(Decimal::ZERO) > estimate.total_cost_bps * dec!(1.5) {
    // Generate optimal schedule
    let scheduler = ExecutionScheduler::new(SchedulerType::ImplementationShortfall {
        risk_aversion: match target.urgency {
            Urgency::Immediate => dec!(0.01),   // Very aggressive
            Urgency::Aggressive => dec!(0.001),
            Urgency::Normal => dec!(0.0001),
            Urgency::Passive => dec!(0.00001),  // Very patient
        },
    });

    let schedule = scheduler.generate_schedule(...);

    // Execute according to schedule
    for slice in schedule.slices {
        // Submit child orders...
    }
}
```

## Performance Grades

Post-trade execution quality grades based on arrival slippage:

| Grade | Slippage Range | Interpretation |
|-------|----------------|----------------|
| **Excellent** | < -5 bps | Beat benchmark |
| **Good** | -5 to 5 bps | On target |
| **Fair** | 5 to 15 bps | Acceptable |
| **Poor** | 15 to 30 bps | Needs improvement |
| **VeryPoor** | > 30 bps | Significant issues |

## Academic References

1. **Kyle, A.S. (1985)**. "Continuous Auctions and Insider Trading." *Econometrica*, 53(6), 1315-1335.

2. **Almgren, R., & Chriss, N. (2000)**. "Optimal execution of portfolio transactions." *Journal of Risk*, 3(2), 5-40.

3. **Bouchaud, J.P., Farmer, J.D., & Lillo, F. (2009)**. "How Markets Slowly Digest Changes in Supply and Demand." In *Handbook of Financial Markets: Dynamics and Evolution*.

4. **Gatheral, J. (2010)**. "No-dynamic-arbitrage and market impact." *Quantitative Finance*, 10(7), 749-759.

## Example: Full TCA Workflow

```rust
use athena_order_manager::tca::*;
use rust_decimal_macros::dec;

// 1. Market State
let market = MarketState::new("BTC-USD")
    .with_bbo(dec!(50000), dec!(50010))
    .with_adv(dec!(10000))
    .with_volatility(dec!(0.50));

// 2. Pre-Trade Estimation
let estimator = TcaEstimator::default();
let order = OrderSpec::new("BTC-USD", true, dec!(500), 3600);
let estimate = estimator.estimate(&order, &market);
println!("Expected cost: {} bps", estimate.total_cost_bps);

// 3. Generate Optimal Schedule
let scheduler = ExecutionScheduler::new(SchedulerType::ImplementationShortfall {
    risk_aversion: dec!(0.0005),
});
let schedule = scheduler.generate_schedule("BTC-USD", true, dec!(500), 3600, 20, &market);
println!("Schedule: {} slices, {} expected cost", schedule.slices.len(), schedule.expected_cost_bps);

// 4. Execute (simulated)
let mut benchmarks = ExecutionBenchmarks::new("BTC-USD", Utc::now());
benchmarks.set_arrival_price(market.mid_price().unwrap());

// ... execute order slices ...

// 5. Post-Trade Measurement
let metrics = TcaMeasurement::measure(
    "order-1", "BTC-USD", dec!(500), true,
    &executions, &benchmarks, Some(&estimate),
);
println!("{}", metrics.summary());

// 6. Compare estimate vs realized
if let Some(error) = metrics.estimate_error_bps {
    println!("Estimate error: {:+.1} bps", error);
}
```
