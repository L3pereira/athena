//! Market Impact Models
//!
//! Implementations of industry-standard market impact models from academic literature.
//!
//! # Models
//!
//! ## Kyle (1985) - Linear Impact
//!
//! The classic model where impact is proportional to order size:
//! ```text
//! Impact = λ × Q
//! ```
//! Where λ (Kyle's lambda) represents price sensitivity to order flow.
//!
//! ## Almgren-Chriss (2000) - Temporary + Permanent Impact
//!
//! Separates impact into temporary (transient) and permanent components:
//! ```text
//! Temporary Impact = η × (Q/T)         # Rate-dependent, decays
//! Permanent Impact = γ × Q             # Permanent information impact
//! Total Impact = Temporary + Permanent
//! ```
//!
//! ## Bouchaud et al. - Square Root Law
//!
//! Empirically validated concave relationship:
//! ```text
//! Impact = σ × √(Q/V) × Y
//! ```
//! Where σ is volatility, V is volume, and Y is a calibrated constant.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::fmt;

use super::MarketState;

/// Type of market impact
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactType {
    /// Temporary impact - decays after execution
    Temporary,
    /// Permanent impact - persists indefinitely
    Permanent,
    /// Total impact (temporary + permanent)
    Total,
}

/// Result of market impact calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketImpact {
    /// Temporary impact in basis points
    pub temporary_bps: Decimal,
    /// Permanent impact in basis points
    pub permanent_bps: Decimal,
    /// Total impact in basis points
    pub total_bps: Decimal,
    /// Impact in absolute price terms (if mid price available)
    pub absolute_impact: Option<Decimal>,
}

impl MarketImpact {
    /// Create from basis points components
    pub fn from_bps(temporary: Decimal, permanent: Decimal) -> Self {
        Self {
            temporary_bps: temporary,
            permanent_bps: permanent,
            total_bps: temporary + permanent,
            absolute_impact: None,
        }
    }

    /// Set absolute impact from mid price
    pub fn with_mid_price(mut self, mid_price: Decimal) -> Self {
        self.absolute_impact = Some(self.total_bps * mid_price / dec!(10000));
        self
    }

    /// Zero impact
    pub fn zero() -> Self {
        Self {
            temporary_bps: Decimal::ZERO,
            permanent_bps: Decimal::ZERO,
            total_bps: Decimal::ZERO,
            absolute_impact: None,
        }
    }
}

/// Trait for market impact models
pub trait ImpactModel: Send + Sync {
    /// Calculate market impact for a given quantity and market state
    fn calculate_impact(&self, quantity: Decimal, market: &MarketState) -> MarketImpact;

    /// Calculate impact for a given trading rate (quantity per unit time)
    fn calculate_rate_impact(
        &self,
        rate: Decimal,
        duration_secs: u64,
        market: &MarketState,
    ) -> MarketImpact;

    /// Model name for logging/display
    fn name(&self) -> &'static str;

    /// Clone into boxed trait object
    fn box_clone(&self) -> Box<dyn ImpactModel>;
}

impl Clone for Box<dyn ImpactModel> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

// ============================================================================
// Kyle (1985) Linear Impact Model
// ============================================================================

/// Parameters for Kyle's linear impact model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KyleParams {
    /// Kyle's lambda - price impact per unit of order flow
    /// Typically expressed as bps per 1% of ADV
    pub lambda: Decimal,
    /// Whether to treat all impact as permanent (classic Kyle)
    pub all_permanent: bool,
    /// Fraction of impact that is temporary (if not all permanent)
    pub temporary_fraction: Decimal,
}

impl Default for KyleParams {
    fn default() -> Self {
        Self {
            lambda: dec!(10), // 10 bps per 1% of ADV
            all_permanent: true,
            temporary_fraction: dec!(0.5),
        }
    }
}

/// Kyle (1985) Linear Impact Model
///
/// Classic model where price impact is linear in order size:
/// `Impact = λ × (Q / ADV)`
#[derive(Debug, Clone)]
pub struct KyleModel {
    params: KyleParams,
}

impl KyleModel {
    /// Create new Kyle model with parameters
    pub fn new(params: KyleParams) -> Self {
        Self { params }
    }

    /// Create with default parameters
    pub fn with_lambda(lambda: Decimal) -> Self {
        Self {
            params: KyleParams {
                lambda,
                ..Default::default()
            },
        }
    }
}

impl ImpactModel for KyleModel {
    fn calculate_impact(&self, quantity: Decimal, market: &MarketState) -> MarketImpact {
        let participation = market.participation_rate(quantity);
        let total_impact_bps = self.params.lambda * participation * dec!(100); // Convert to bps

        let (temporary, permanent) = if self.params.all_permanent {
            (Decimal::ZERO, total_impact_bps)
        } else {
            let temp = total_impact_bps * self.params.temporary_fraction;
            (temp, total_impact_bps - temp)
        };

        let mut impact = MarketImpact::from_bps(temporary, permanent);
        if let Some(mid) = market.mid_price() {
            impact = impact.with_mid_price(mid);
        }
        impact
    }

    fn calculate_rate_impact(
        &self,
        rate: Decimal,
        duration_secs: u64,
        market: &MarketState,
    ) -> MarketImpact {
        let total_quantity = rate * Decimal::from(duration_secs);
        self.calculate_impact(total_quantity, market)
    }

    fn name(&self) -> &'static str {
        "Kyle (1985)"
    }

    fn box_clone(&self) -> Box<dyn ImpactModel> {
        Box::new(self.clone())
    }
}

impl fmt::Display for KyleModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Kyle(λ={})", self.params.lambda)
    }
}

// ============================================================================
// Almgren-Chriss (2000) Temporary + Permanent Impact Model
// ============================================================================

/// Parameters for Almgren-Chriss model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlmgrenChrissParams {
    /// Permanent impact coefficient γ (gamma)
    /// Permanent impact = γ × Q
    pub gamma: Decimal,
    /// Temporary impact coefficient η (eta)
    /// Temporary impact = η × trading_rate
    pub eta: Decimal,
    /// Temporary impact exponent (typically 0.5-1.0)
    /// For linear: 1.0, for square-root: 0.5
    pub eta_exponent: Decimal,
    /// Volatility scaling factor
    pub volatility_scale: Decimal,
    /// ADV scaling exponent for normalization
    pub adv_exponent: Decimal,
}

impl Default for AlmgrenChrissParams {
    fn default() -> Self {
        Self {
            gamma: dec!(0.1),          // Permanent impact coefficient
            eta: dec!(0.05),           // Temporary impact coefficient
            eta_exponent: dec!(0.6),   // Slightly concave (between linear and sqrt)
            volatility_scale: dec!(1), // Scale by volatility
            adv_exponent: dec!(0.5),   // Normalize by sqrt(ADV)
        }
    }
}

impl AlmgrenChrissParams {
    /// Create parameters calibrated for high-liquidity assets
    pub fn high_liquidity() -> Self {
        Self {
            gamma: dec!(0.05),
            eta: dec!(0.02),
            eta_exponent: dec!(0.5),
            volatility_scale: dec!(0.8),
            adv_exponent: dec!(0.5),
        }
    }

    /// Create parameters calibrated for low-liquidity assets
    pub fn low_liquidity() -> Self {
        Self {
            gamma: dec!(0.3),
            eta: dec!(0.15),
            eta_exponent: dec!(0.7),
            volatility_scale: dec!(1.2),
            adv_exponent: dec!(0.5),
        }
    }

    /// Create parameters for crypto markets
    pub fn crypto() -> Self {
        Self {
            gamma: dec!(0.15),
            eta: dec!(0.08),
            eta_exponent: dec!(0.55),
            volatility_scale: dec!(1.5), // Higher vol in crypto
            adv_exponent: dec!(0.5),
        }
    }
}

/// Almgren-Chriss (2000) Impact Model
///
/// Separates market impact into temporary and permanent components:
///
/// - **Permanent Impact**: Information leakage that moves the equilibrium price
///   `g(Q) = γ × Q` (linear in total quantity)
///
/// - **Temporary Impact**: Execution pressure that decays after trading
///   `h(v) = η × v^α` (function of trading rate)
///
/// The optimal execution trajectory minimizes:
/// `E[Cost] + λ × Var[Cost]`
#[derive(Debug, Clone)]
pub struct AlmgrenChrissModel {
    params: AlmgrenChrissParams,
}

impl AlmgrenChrissModel {
    /// Create new Almgren-Chriss model with parameters
    pub fn new(params: AlmgrenChrissParams) -> Self {
        Self { params }
    }

    /// Create with default parameters
    pub fn default_model() -> Self {
        Self {
            params: AlmgrenChrissParams::default(),
        }
    }

    /// Get model parameters
    pub fn params(&self) -> &AlmgrenChrissParams {
        &self.params
    }

    /// Calculate optimal trading trajectory for given risk aversion
    ///
    /// Returns the fraction of total quantity to trade at each time step
    /// based on the Almgren-Chriss optimal solution.
    pub fn optimal_trajectory(
        &self,
        total_quantity: Decimal,
        num_periods: usize,
        risk_aversion: Decimal,
        market: &MarketState,
    ) -> Vec<Decimal> {
        if num_periods == 0 {
            return vec![];
        }

        // Simplified Almgren-Chriss optimal solution
        // x_j = X * sinh(κ(T-t_j)) / sinh(κT)
        // where κ = sqrt(λσ²/η)

        let sigma = market.volatility;
        let eta = self.params.eta;

        // Calculate κ (kappa)
        let lambda_sigma_sq = risk_aversion * sigma * sigma;
        let kappa_sq = if eta > Decimal::ZERO {
            lambda_sigma_sq / eta
        } else {
            dec!(0.01)
        };
        let kappa = sqrt_decimal(kappa_sq);

        let t_max = Decimal::from(num_periods);
        let sinh_kt = sinh_decimal(kappa * t_max);

        let mut trajectory = Vec::with_capacity(num_periods);

        for j in 0..num_periods {
            let t_j = Decimal::from(j);
            let remaining_time = t_max - t_j;
            let sinh_remaining = sinh_decimal(kappa * remaining_time);

            let fraction = if sinh_kt > Decimal::ZERO {
                sinh_remaining / sinh_kt
            } else {
                // Fall back to linear if sinh is zero
                remaining_time / t_max
            };

            // Trading rate for this period (difference in inventory)
            let x_j = total_quantity * fraction;
            let x_next = if j + 1 < num_periods {
                let next_remaining = t_max - Decimal::from(j + 1);
                let sinh_next = sinh_decimal(kappa * next_remaining);
                total_quantity * sinh_next / sinh_kt.max(dec!(0.0001))
            } else {
                Decimal::ZERO
            };

            trajectory.push(x_j - x_next);
        }

        trajectory
    }
}

impl ImpactModel for AlmgrenChrissModel {
    fn calculate_impact(&self, quantity: Decimal, market: &MarketState) -> MarketImpact {
        // Normalize quantity by ADV
        let adv = market.adv.max(dec!(1));
        let normalized_qty = quantity / pow_decimal(adv, self.params.adv_exponent);

        // Permanent impact: γ × normalized_quantity × volatility
        let permanent = self.params.gamma
            * normalized_qty
            * market.volatility
            * self.params.volatility_scale
            * dec!(10000); // Convert to bps

        // Temporary impact depends on trading rate
        // For full-quantity calculation, assume instant execution
        let temporary = self.params.eta
            * pow_decimal(normalized_qty, self.params.eta_exponent)
            * market.volatility
            * self.params.volatility_scale
            * dec!(10000); // Convert to bps

        let mut impact = MarketImpact::from_bps(temporary, permanent);
        if let Some(mid) = market.mid_price() {
            impact = impact.with_mid_price(mid);
        }
        impact
    }

    fn calculate_rate_impact(
        &self,
        rate: Decimal,
        duration_secs: u64,
        market: &MarketState,
    ) -> MarketImpact {
        let total_quantity = rate * Decimal::from(duration_secs);
        let adv = market.adv.max(dec!(1));

        // Normalize quantities
        let normalized_qty = total_quantity / pow_decimal(adv, self.params.adv_exponent);
        let normalized_rate =
            rate / pow_decimal(adv / Decimal::from(86400), self.params.adv_exponent); // Daily rate

        // Permanent impact (from total quantity)
        let permanent = self.params.gamma
            * normalized_qty
            * market.volatility
            * self.params.volatility_scale
            * dec!(10000);

        // Temporary impact (from rate)
        let temporary = self.params.eta
            * pow_decimal(normalized_rate, self.params.eta_exponent)
            * market.volatility
            * self.params.volatility_scale
            * dec!(10000);

        let mut impact = MarketImpact::from_bps(temporary, permanent);
        if let Some(mid) = market.mid_price() {
            impact = impact.with_mid_price(mid);
        }
        impact
    }

    fn name(&self) -> &'static str {
        "Almgren-Chriss (2000)"
    }

    fn box_clone(&self) -> Box<dyn ImpactModel> {
        Box::new(self.clone())
    }
}

impl fmt::Display for AlmgrenChrissModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AlmgrenChriss(γ={}, η={}, α={})",
            self.params.gamma, self.params.eta, self.params.eta_exponent
        )
    }
}

// ============================================================================
// Square Root (Bouchaud et al.) Impact Model
// ============================================================================

/// Parameters for Square Root impact model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquareRootParams {
    /// Impact coefficient Y (typically 0.1 - 0.5)
    /// Impact = Y × σ × √(Q/V)
    pub y_coefficient: Decimal,
    /// Fraction of impact that is temporary vs permanent
    /// 0.0 = all permanent, 1.0 = all temporary
    pub temporary_fraction: Decimal,
    /// Decay half-life for temporary impact (in seconds)
    pub decay_half_life_secs: u64,
}

impl Default for SquareRootParams {
    fn default() -> Self {
        Self {
            y_coefficient: dec!(0.3),      // Typical empirical value
            temporary_fraction: dec!(0.7), // Most impact is temporary
            decay_half_life_secs: 300,     // 5 minute half-life
        }
    }
}

impl SquareRootParams {
    /// Parameters calibrated from Bouchaud et al. empirical studies
    pub fn bouchaud_empirical() -> Self {
        Self {
            y_coefficient: dec!(0.314), // ~1/π, found empirically
            temporary_fraction: dec!(0.65),
            decay_half_life_secs: 600, // 10 minutes
        }
    }

    /// Conservative parameters (higher impact)
    pub fn conservative() -> Self {
        Self {
            y_coefficient: dec!(0.5),
            temporary_fraction: dec!(0.6),
            decay_half_life_secs: 180,
        }
    }
}

/// Square Root Impact Model (Bouchaud et al.)
///
/// Empirically validated model where impact scales with square root of participation:
///
/// `Impact = Y × σ × √(Q/V)`
///
/// Where:
/// - Y: calibrated constant (~0.3 empirically)
/// - σ: daily volatility
/// - Q: order quantity
/// - V: average daily volume
///
/// This concave relationship captures the empirical observation that
/// large orders have diminishing marginal impact.
#[derive(Debug, Clone)]
pub struct SquareRootModel {
    params: SquareRootParams,
}

impl SquareRootModel {
    /// Create new Square Root model with parameters
    pub fn new(params: SquareRootParams) -> Self {
        Self { params }
    }

    /// Create with default parameters
    pub fn default_model() -> Self {
        Self {
            params: SquareRootParams::default(),
        }
    }

    /// Get model parameters
    pub fn params(&self) -> &SquareRootParams {
        &self.params
    }

    /// Calculate decayed impact after given time
    pub fn decayed_temporary_impact(
        &self,
        initial_impact_bps: Decimal,
        elapsed_secs: u64,
    ) -> Decimal {
        let half_life = self.params.decay_half_life_secs as f64;
        let elapsed = elapsed_secs as f64;
        let decay_factor = 0.5_f64.powf(elapsed / half_life);
        initial_impact_bps * Decimal::try_from(decay_factor).unwrap_or(Decimal::ZERO)
    }
}

impl ImpactModel for SquareRootModel {
    fn calculate_impact(&self, quantity: Decimal, market: &MarketState) -> MarketImpact {
        let participation = market.participation_rate(quantity);

        // Impact = Y × σ × √(participation) × 10000 (to bps)
        let sqrt_participation = sqrt_decimal(participation);
        let total_impact_bps =
            self.params.y_coefficient * market.volatility * sqrt_participation * dec!(10000);

        // Split into temporary and permanent
        let temporary = total_impact_bps * self.params.temporary_fraction;
        let permanent = total_impact_bps * (Decimal::ONE - self.params.temporary_fraction);

        let mut impact = MarketImpact::from_bps(temporary, permanent);
        if let Some(mid) = market.mid_price() {
            impact = impact.with_mid_price(mid);
        }
        impact
    }

    fn calculate_rate_impact(
        &self,
        rate: Decimal,
        duration_secs: u64,
        market: &MarketState,
    ) -> MarketImpact {
        // For rate-based calculation, use instantaneous impact
        // scaled by the square root of trading intensity
        let total_quantity = rate * Decimal::from(duration_secs);
        self.calculate_impact(total_quantity, market)
    }

    fn name(&self) -> &'static str {
        "Square Root (Bouchaud)"
    }

    fn box_clone(&self) -> Box<dyn ImpactModel> {
        Box::new(self.clone())
    }
}

impl fmt::Display for SquareRootModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SquareRoot(Y={}, temp_frac={})",
            self.params.y_coefficient, self.params.temporary_fraction
        )
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Square root approximation for Decimal using Newton's method
pub fn sqrt_decimal(x: Decimal) -> Decimal {
    if x <= Decimal::ZERO {
        return Decimal::ZERO;
    }

    // Newton's method: x_{n+1} = (x_n + S/x_n) / 2
    let mut guess = x / Decimal::TWO;
    if guess.is_zero() {
        guess = dec!(0.0001);
    }

    for _ in 0..10 {
        let new_guess = (guess + x / guess) / Decimal::TWO;
        if (new_guess - guess).abs() < dec!(0.0000001) {
            return new_guess;
        }
        guess = new_guess;
    }
    guess
}

/// Power function approximation for Decimal
pub fn pow_decimal(base: Decimal, exp: Decimal) -> Decimal {
    if base <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    if exp == Decimal::ZERO {
        return Decimal::ONE;
    }
    if exp == Decimal::ONE {
        return base;
    }
    if exp == dec!(0.5) {
        return sqrt_decimal(base);
    }

    // For other exponents, use exp(exp * ln(base))
    // Approximation using Taylor series
    let base_f64 = base.to_string().parse::<f64>().unwrap_or(1.0);
    let exp_f64 = exp.to_string().parse::<f64>().unwrap_or(1.0);
    let result = base_f64.powf(exp_f64);
    Decimal::try_from(result).unwrap_or(Decimal::ONE)
}

/// Hyperbolic sine approximation for Decimal
fn sinh_decimal(x: Decimal) -> Decimal {
    // sinh(x) = (e^x - e^-x) / 2
    let x_f64 = x.to_string().parse::<f64>().unwrap_or(0.0);
    let result = x_f64.sinh();
    Decimal::try_from(result).unwrap_or(Decimal::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_market() -> MarketState {
        MarketState::new("BTC-USD")
            .with_bbo(dec!(50000), dec!(50010))
            .with_adv(dec!(10000))
            .with_volatility(dec!(0.50)) // 50% annual vol
    }

    #[test]
    fn test_kyle_model() {
        let model = KyleModel::with_lambda(dec!(10));
        let market = make_market();

        // 1% of ADV
        let impact = model.calculate_impact(dec!(100), &market);

        assert!(impact.permanent_bps > Decimal::ZERO);
        println!("Kyle impact for 1% ADV: {} bps", impact.total_bps);
    }

    #[test]
    fn test_almgren_chriss_model() {
        let model = AlmgrenChrissModel::default_model();
        let market = make_market();

        let impact = model.calculate_impact(dec!(100), &market);

        assert!(impact.temporary_bps > Decimal::ZERO);
        assert!(impact.permanent_bps > Decimal::ZERO);
        println!(
            "AC impact: temp={} bps, perm={} bps",
            impact.temporary_bps, impact.permanent_bps
        );
    }

    #[test]
    fn test_almgren_chriss_trajectory() {
        let model = AlmgrenChrissModel::default_model();
        let market = make_market();

        let trajectory = model.optimal_trajectory(
            dec!(1000),  // Total quantity
            10,          // 10 periods
            dec!(0.001), // Risk aversion
            &market,
        );

        assert_eq!(trajectory.len(), 10);
        let total: Decimal = trajectory.iter().sum();
        // Total should approximately equal original quantity
        assert!((total - dec!(1000)).abs() < dec!(1));
        println!("Trajectory: {:?}", trajectory);
    }

    #[test]
    fn test_square_root_model() {
        let model = SquareRootModel::default_model();
        let market = make_market();

        // Small order
        let small_impact = model.calculate_impact(dec!(10), &market);
        // Large order
        let large_impact = model.calculate_impact(dec!(1000), &market);

        // Large order impact should be less than 10x small order (concave)
        assert!(large_impact.total_bps < small_impact.total_bps * dec!(10));
        println!(
            "Square root: small={} bps, large={} bps",
            small_impact.total_bps, large_impact.total_bps
        );
    }

    #[test]
    fn test_impact_decay() {
        let model = SquareRootModel::new(SquareRootParams {
            decay_half_life_secs: 60,
            ..Default::default()
        });

        let initial = dec!(100);
        let after_one_halflife = model.decayed_temporary_impact(initial, 60);
        let after_two_halflives = model.decayed_temporary_impact(initial, 120);

        assert!((after_one_halflife - dec!(50)).abs() < dec!(1));
        assert!((after_two_halflives - dec!(25)).abs() < dec!(1));
    }

    #[test]
    fn test_sqrt_decimal() {
        assert!((sqrt_decimal(dec!(4)) - dec!(2)).abs() < dec!(0.0001));
        assert!((sqrt_decimal(dec!(100)) - dec!(10)).abs() < dec!(0.0001));
        assert!((sqrt_decimal(dec!(0.25)) - dec!(0.5)).abs() < dec!(0.0001));
    }
}
