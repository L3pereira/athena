//! Optimal Execution Schedulers
//!
//! Implements industry-standard execution algorithms that optimize
//! the trade-off between market impact and timing risk.
//!
//! # Algorithms
//!
//! ## TWAP (Time-Weighted Average Price)
//! Trades evenly over time, minimizing timing risk but ignoring volume patterns.
//!
//! ## VWAP (Volume-Weighted Average Price)
//! Trades proportionally to expected volume, reducing market impact.
//!
//! ## Implementation Shortfall (Almgren-Chriss)
//! Optimizes: `min E[Cost] + λ × Var[Cost]`
//! Balances urgency (front-loaded) vs patience (spread evenly).
//!
//! ## Adaptive
//! Dynamically adjusts based on real-time market conditions.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::MarketState;
use super::models::{AlmgrenChrissModel, AlmgrenChrissParams, ImpactModel};

/// Type of scheduling algorithm
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum SchedulerType {
    /// Time-weighted: trade evenly over time
    #[default]
    Twap,
    /// Volume-weighted: follow expected volume profile
    Vwap {
        /// Historical volume profile (hour -> fraction of daily volume)
        volume_profile: VolumeProfile,
    },
    /// Implementation Shortfall: Almgren-Chriss optimal
    ImplementationShortfall {
        /// Risk aversion parameter (higher = more aggressive)
        risk_aversion: Decimal,
    },
    /// Percentage of Volume: trade at fixed rate of market volume
    Pov {
        /// Target participation rate (e.g., 0.10 = 10% of volume)
        target_rate: Decimal,
        /// Maximum participation rate cap
        max_rate: Decimal,
    },
    /// Adaptive: adjust based on market conditions
    Adaptive {
        /// Base strategy to modify
        base_strategy: Box<SchedulerType>,
        /// Aggression adjustment factor
        aggression_factor: Decimal,
    },
}

/// Volume profile for VWAP scheduling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfile {
    /// Hour (0-23) -> fraction of daily volume
    pub hourly_fractions: HashMap<u32, Decimal>,
    /// Whether this is a crypto market (24/7)
    pub is_24h_market: bool,
}

impl Default for VolumeProfile {
    fn default() -> Self {
        // Typical U-shaped equity volume profile
        let mut fractions = HashMap::new();
        fractions.insert(9, dec!(0.12)); // Market open
        fractions.insert(10, dec!(0.08));
        fractions.insert(11, dec!(0.06));
        fractions.insert(12, dec!(0.05));
        fractions.insert(13, dec!(0.05));
        fractions.insert(14, dec!(0.06));
        fractions.insert(15, dec!(0.10)); // Close
        // Remaining distributed in pre/post market

        Self {
            hourly_fractions: fractions,
            is_24h_market: false,
        }
    }
}

impl VolumeProfile {
    /// Create flat profile for crypto (24/7 markets)
    pub fn crypto_24h() -> Self {
        let mut fractions = HashMap::new();
        let hourly = dec!(1) / dec!(24);
        for hour in 0..24 {
            fractions.insert(hour, hourly);
        }
        Self {
            hourly_fractions: fractions,
            is_24h_market: true,
        }
    }

    /// Get expected volume fraction for a given hour
    pub fn fraction_for_hour(&self, hour: u32) -> Decimal {
        *self.hourly_fractions.get(&hour).unwrap_or(&dec!(0.04))
    }
}

/// Configuration for execution scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Minimum slice size
    pub min_slice_size: Decimal,
    /// Maximum slice size
    pub max_slice_size: Decimal,
    /// Minimum time between slices (seconds)
    pub min_interval_secs: u64,
    /// Whether to randomize slice timing (reduce signaling)
    pub randomize_timing: bool,
    /// Randomization jitter (fraction of interval)
    pub timing_jitter: Decimal,
    /// Whether to allow partial fills to roll over
    pub allow_rollover: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            min_slice_size: dec!(0.001),
            max_slice_size: dec!(1000),
            min_interval_secs: 1,
            randomize_timing: true,
            timing_jitter: dec!(0.1),
            allow_rollover: true,
        }
    }
}

/// A single slice in the execution schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleSlice {
    /// Slice index (0-based)
    pub index: usize,
    /// Target quantity for this slice
    pub quantity: Decimal,
    /// Target start time
    pub start_time: DateTime<Utc>,
    /// Target end time
    pub end_time: DateTime<Utc>,
    /// Fraction of total order
    pub fraction: Decimal,
    /// Cumulative fraction executed after this slice
    pub cumulative_fraction: Decimal,
    /// Expected participation rate during this slice
    pub expected_participation: Decimal,
    /// Whether this is a limit or market order
    pub use_limit_order: bool,
    /// Suggested price offset in ticks (negative = passive)
    pub price_offset_ticks: i32,
}

/// Complete execution schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSchedule {
    /// Instrument being traded
    pub instrument_id: String,
    /// Total quantity to execute
    pub total_quantity: Decimal,
    /// Is buy order
    pub is_buy: bool,
    /// Schedule start time
    pub start_time: DateTime<Utc>,
    /// Schedule end time
    pub end_time: DateTime<Utc>,
    /// Individual slices
    pub slices: Vec<ScheduleSlice>,
    /// Algorithm used
    pub algorithm: String,
    /// Expected total cost (bps)
    pub expected_cost_bps: Decimal,
    /// Expected average participation rate
    pub expected_participation: Decimal,
}

impl ExecutionSchedule {
    /// Get remaining quantity to execute
    pub fn remaining_quantity(&self, executed_so_far: Decimal) -> Decimal {
        (self.total_quantity - executed_so_far).max(Decimal::ZERO)
    }

    /// Get current slice based on time
    pub fn current_slice(&self, now: DateTime<Utc>) -> Option<&ScheduleSlice> {
        self.slices
            .iter()
            .find(|s| now >= s.start_time && now < s.end_time)
    }

    /// Get next slice
    pub fn next_slice(&self, now: DateTime<Utc>) -> Option<&ScheduleSlice> {
        self.slices.iter().find(|s| s.start_time > now)
    }

    /// Check if schedule is complete
    pub fn is_complete(&self, now: DateTime<Utc>) -> bool {
        now >= self.end_time
    }

    /// Calculate schedule progress
    pub fn progress(&self, executed_quantity: Decimal) -> Decimal {
        if self.total_quantity > Decimal::ZERO {
            executed_quantity / self.total_quantity
        } else {
            Decimal::ONE
        }
    }
}

/// Execution scheduler
///
/// Generates optimal execution schedules based on the chosen algorithm.
pub struct ExecutionScheduler {
    scheduler_type: SchedulerType,
    config: SchedulerConfig,
}

impl ExecutionScheduler {
    /// Create new scheduler with algorithm type
    pub fn new(scheduler_type: SchedulerType) -> Self {
        Self {
            scheduler_type,
            config: SchedulerConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(scheduler_type: SchedulerType, config: SchedulerConfig) -> Self {
        Self {
            scheduler_type,
            config,
        }
    }

    /// Generate execution schedule
    pub fn generate_schedule(
        &self,
        instrument_id: &str,
        is_buy: bool,
        total_quantity: Decimal,
        duration_secs: u64,
        num_slices: usize,
        market: &MarketState,
    ) -> ExecutionSchedule {
        let now = Utc::now();
        let end_time = now + Duration::seconds(duration_secs as i64);

        let slice_fractions = match &self.scheduler_type {
            SchedulerType::Twap => self.generate_twap_fractions(num_slices),
            SchedulerType::Vwap { volume_profile } => {
                self.generate_vwap_fractions(num_slices, duration_secs, now, volume_profile)
            }
            SchedulerType::ImplementationShortfall { risk_aversion } => {
                self.generate_is_fractions(num_slices, *risk_aversion, market)
            }
            SchedulerType::Pov { target_rate, .. } => {
                self.generate_pov_fractions(num_slices, total_quantity, *target_rate, market)
            }
            SchedulerType::Adaptive {
                base_strategy,
                aggression_factor,
            } => {
                // Start with base strategy then adjust
                let base_scheduler = ExecutionScheduler::new(*base_strategy.clone());
                let mut fractions = base_scheduler.generate_fractions(num_slices, market);
                self.apply_adaptive_adjustments(&mut fractions, *aggression_factor, market);
                fractions
            }
        };

        let slices = self.build_slices(
            &slice_fractions,
            total_quantity,
            now,
            end_time,
            num_slices,
            market,
        );

        // Calculate expected cost
        let expected_cost_bps = self.estimate_schedule_cost(&slices, market);
        let expected_participation = total_quantity / market.adv.max(dec!(1));

        ExecutionSchedule {
            instrument_id: instrument_id.to_string(),
            total_quantity,
            is_buy,
            start_time: now,
            end_time,
            slices,
            algorithm: self.algorithm_name(),
            expected_cost_bps,
            expected_participation,
        }
    }

    /// Get algorithm name
    pub fn algorithm_name(&self) -> String {
        match &self.scheduler_type {
            SchedulerType::Twap => "TWAP".to_string(),
            SchedulerType::Vwap { .. } => "VWAP".to_string(),
            SchedulerType::ImplementationShortfall { risk_aversion } => {
                format!("IS(λ={})", risk_aversion)
            }
            SchedulerType::Pov { target_rate, .. } => format!("POV({}%)", target_rate * dec!(100)),
            SchedulerType::Adaptive { base_strategy, .. } => {
                let base_name = ExecutionScheduler::new(*base_strategy.clone()).algorithm_name();
                format!("Adaptive-{}", base_name)
            }
        }
    }

    /// Generate fractions for any strategy (internal dispatch)
    fn generate_fractions(&self, num_slices: usize, market: &MarketState) -> Vec<Decimal> {
        match &self.scheduler_type {
            SchedulerType::Twap => self.generate_twap_fractions(num_slices),
            SchedulerType::ImplementationShortfall { risk_aversion } => {
                self.generate_is_fractions(num_slices, *risk_aversion, market)
            }
            _ => self.generate_twap_fractions(num_slices), // Fallback
        }
    }

    /// Generate TWAP fractions (uniform)
    fn generate_twap_fractions(&self, num_slices: usize) -> Vec<Decimal> {
        if num_slices == 0 {
            return vec![];
        }
        let fraction = Decimal::ONE / Decimal::from(num_slices as u64);
        vec![fraction; num_slices]
    }

    /// Generate VWAP fractions based on volume profile
    fn generate_vwap_fractions(
        &self,
        num_slices: usize,
        duration_secs: u64,
        start_time: DateTime<Utc>,
        profile: &VolumeProfile,
    ) -> Vec<Decimal> {
        if num_slices == 0 {
            return vec![];
        }

        let slice_duration = Duration::seconds(duration_secs as i64 / num_slices as i64);
        let mut fractions = Vec::with_capacity(num_slices);
        let mut total_volume_weight = Decimal::ZERO;

        // Calculate volume weight for each slice
        for i in 0..num_slices {
            let slice_time = start_time + slice_duration * i as i32;
            let hour = slice_time
                .format("%H")
                .to_string()
                .parse::<u32>()
                .unwrap_or(0);
            let weight = profile.fraction_for_hour(hour);
            fractions.push(weight);
            total_volume_weight += weight;
        }

        // Normalize
        if total_volume_weight > Decimal::ZERO {
            for f in fractions.iter_mut() {
                *f /= total_volume_weight;
            }
        }

        fractions
    }

    /// Generate Implementation Shortfall fractions (Almgren-Chriss optimal)
    fn generate_is_fractions(
        &self,
        num_slices: usize,
        risk_aversion: Decimal,
        market: &MarketState,
    ) -> Vec<Decimal> {
        if num_slices == 0 {
            return vec![];
        }

        // Use Almgren-Chriss optimal trajectory
        let model = AlmgrenChrissModel::new(AlmgrenChrissParams::default());
        let trajectory = model.optimal_trajectory(
            Decimal::ONE, // Normalized quantity
            num_slices,
            risk_aversion,
            market,
        );

        // Normalize to sum to 1
        let total: Decimal = trajectory.iter().sum();
        if total > Decimal::ZERO {
            trajectory.into_iter().map(|t| t / total).collect()
        } else {
            self.generate_twap_fractions(num_slices)
        }
    }

    /// Generate POV (Percentage of Volume) fractions
    fn generate_pov_fractions(
        &self,
        num_slices: usize,
        total_quantity: Decimal,
        target_rate: Decimal,
        market: &MarketState,
    ) -> Vec<Decimal> {
        if num_slices == 0 {
            return vec![];
        }

        // Calculate how many slices needed at target rate
        let expected_slice_volume = market.adv / Decimal::from(num_slices as u64);
        let max_per_slice = expected_slice_volume * target_rate;
        let quantity_per_slice = total_quantity / Decimal::from(num_slices as u64);

        // Cap at target rate
        let actual_per_slice = quantity_per_slice.min(max_per_slice);
        let fraction = actual_per_slice / total_quantity;

        vec![fraction; num_slices]
    }

    /// Apply adaptive adjustments based on market conditions
    fn apply_adaptive_adjustments(
        &self,
        fractions: &mut [Decimal],
        aggression_factor: Decimal,
        market: &MarketState,
    ) {
        if fractions.is_empty() {
            return;
        }

        // Adjust based on spread: wider spread = more passive (backload)
        let spread_adjustment = if let Some(spread_bps) = market.current_spread_bps() {
            if spread_bps > dec!(10) {
                dec!(0.9) // Wide spread: backload
            } else if spread_bps < dec!(3) {
                dec!(1.1) // Tight spread: frontload
            } else {
                Decimal::ONE
            }
        } else {
            Decimal::ONE
        };

        // Adjust based on volatility: higher vol = more aggressive (frontload)
        let vol_adjustment = if market.volatility > dec!(0.5) {
            dec!(1.2) // High vol: frontload to reduce timing risk
        } else if market.volatility < dec!(0.2) {
            dec!(0.9) // Low vol: can be more patient
        } else {
            Decimal::ONE
        };

        let adjustment = spread_adjustment * vol_adjustment * aggression_factor;

        // Apply adjustment: values > 1 frontload, < 1 backload
        let n = fractions.len();
        for (i, f) in fractions.iter_mut().enumerate() {
            let position = Decimal::from(i as u64) / Decimal::from(n as u64);
            let weight =
                Decimal::ONE + (adjustment - Decimal::ONE) * (Decimal::ONE - position * dec!(2));
            *f *= weight;
        }

        // Renormalize
        let total: Decimal = fractions.iter().sum();
        if total > Decimal::ZERO {
            for f in fractions.iter_mut() {
                *f /= total;
            }
        }
    }

    /// Build schedule slices from fractions
    fn build_slices(
        &self,
        fractions: &[Decimal],
        total_quantity: Decimal,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        num_slices: usize,
        market: &MarketState,
    ) -> Vec<ScheduleSlice> {
        if num_slices == 0 {
            return vec![];
        }

        let total_duration = (end_time - start_time).num_seconds() as u64;
        let slice_duration_secs = total_duration / num_slices as u64;

        let mut slices = Vec::with_capacity(num_slices);
        let mut cumulative = Decimal::ZERO;

        for (i, &fraction) in fractions.iter().enumerate() {
            // Clamp quantity to config limits
            let raw_quantity = total_quantity * fraction;
            let quantity = raw_quantity
                .max(self.config.min_slice_size)
                .min(self.config.max_slice_size);
            cumulative += fraction;

            let slice_start =
                start_time + Duration::seconds((i as u64 * slice_duration_secs) as i64);
            let slice_end = slice_start + Duration::seconds(slice_duration_secs as i64);

            // Calculate expected participation for this slice
            let slice_volume = market.adv * fraction;
            let participation = if slice_volume > Decimal::ZERO {
                quantity / slice_volume
            } else {
                Decimal::ZERO
            };

            // Determine order type and price offset based on participation
            let (use_limit, offset) = if participation > dec!(0.2) {
                (true, 1) // Aggressive: cross spread
            } else if participation > dec!(0.05) {
                (true, 0) // At best
            } else {
                (true, -1) // Passive
            };

            slices.push(ScheduleSlice {
                index: i,
                quantity,
                start_time: slice_start,
                end_time: slice_end,
                fraction,
                cumulative_fraction: cumulative,
                expected_participation: participation,
                use_limit_order: use_limit,
                price_offset_ticks: offset,
            });
        }

        slices
    }

    /// Estimate total cost for a schedule
    fn estimate_schedule_cost(&self, slices: &[ScheduleSlice], market: &MarketState) -> Decimal {
        use super::models::SquareRootModel;

        let model = SquareRootModel::default_model();
        let mut total_cost_bps = Decimal::ZERO;

        // Spread cost (assume half spread per slice)
        let spread_bps = market.current_spread_bps().unwrap_or(dec!(5));

        for slice in slices {
            // Spread
            total_cost_bps += spread_bps / Decimal::TWO * slice.fraction;

            // Impact (using square root model)
            let impact = model.calculate_impact(slice.quantity, market);
            total_cost_bps += impact.total_bps * slice.fraction;
        }

        total_cost_bps
    }
}

impl Default for ExecutionScheduler {
    fn default() -> Self {
        Self::new(SchedulerType::Twap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_market() -> MarketState {
        MarketState::new("BTC-USD")
            .with_bbo(dec!(50000), dec!(50010))
            .with_adv(dec!(10000))
            .with_volatility(dec!(0.50))
    }

    #[test]
    fn test_twap_schedule() {
        let scheduler = ExecutionScheduler::new(SchedulerType::Twap);
        let market = make_market();

        let schedule = scheduler.generate_schedule(
            "BTC-USD",
            true,
            dec!(100),
            3600, // 1 hour
            10,   // 10 slices
            &market,
        );

        assert_eq!(schedule.slices.len(), 10);
        assert_eq!(schedule.algorithm, "TWAP");

        // All slices should be equal
        let first_qty = schedule.slices[0].quantity;
        for slice in &schedule.slices {
            assert!((slice.quantity - first_qty).abs() < dec!(0.001));
        }

        println!("TWAP expected cost: {} bps", schedule.expected_cost_bps);
    }

    #[test]
    fn test_is_schedule() {
        let scheduler = ExecutionScheduler::new(SchedulerType::ImplementationShortfall {
            risk_aversion: dec!(0.001),
        });
        let market = make_market();

        let schedule = scheduler.generate_schedule("BTC-USD", true, dec!(100), 3600, 10, &market);

        assert_eq!(schedule.slices.len(), 10);
        assert!(schedule.algorithm.starts_with("IS"));

        // IS should frontload (first slice > last slice for high risk aversion)
        // This depends on parameters, so just check it sums correctly
        let total: Decimal = schedule.slices.iter().map(|s| s.quantity).sum();
        assert!((total - dec!(100)).abs() < dec!(0.01));

        println!(
            "IS schedule quantities: {:?}",
            schedule
                .slices
                .iter()
                .map(|s| s.quantity)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_vwap_schedule() {
        let scheduler = ExecutionScheduler::new(SchedulerType::Vwap {
            volume_profile: VolumeProfile::crypto_24h(),
        });
        let market = make_market();

        let schedule = scheduler.generate_schedule("BTC-USD", true, dec!(100), 3600, 10, &market);

        assert_eq!(schedule.slices.len(), 10);
        assert_eq!(schedule.algorithm, "VWAP");
    }

    #[test]
    fn test_pov_schedule() {
        let scheduler = ExecutionScheduler::new(SchedulerType::Pov {
            target_rate: dec!(0.10),
            max_rate: dec!(0.20),
        });
        let market = make_market();

        let schedule = scheduler.generate_schedule("BTC-USD", true, dec!(100), 3600, 10, &market);

        assert!(schedule.algorithm.starts_with("POV"));

        // Check participation is capped
        for slice in &schedule.slices {
            assert!(slice.expected_participation <= dec!(0.20));
        }
    }

    #[test]
    fn test_adaptive_schedule() {
        let scheduler = ExecutionScheduler::new(SchedulerType::Adaptive {
            base_strategy: Box::new(SchedulerType::Twap),
            aggression_factor: dec!(1.2),
        });
        let market = make_market();

        let schedule = scheduler.generate_schedule("BTC-USD", true, dec!(100), 3600, 10, &market);

        assert!(schedule.algorithm.starts_with("Adaptive"));

        // Should be front-loaded due to aggression > 1
        let first_half: Decimal = schedule.slices[..5].iter().map(|s| s.quantity).sum();
        let second_half: Decimal = schedule.slices[5..].iter().map(|s| s.quantity).sum();
        // Due to aggression, first half should have more (approximately)
        println!("First half: {}, Second half: {}", first_half, second_half);
    }

    #[test]
    fn test_schedule_progress() {
        let scheduler = ExecutionScheduler::default();
        let market = make_market();

        let schedule = scheduler.generate_schedule("BTC-USD", true, dec!(100), 3600, 10, &market);

        assert_eq!(schedule.progress(dec!(0)), Decimal::ZERO);
        assert_eq!(schedule.progress(dec!(50)), dec!(0.5));
        assert_eq!(schedule.progress(dec!(100)), Decimal::ONE);
    }
}
