//! Avellaneda-Stoikov Optimal Market Making
//!
//! From Avellaneda & Stoikov (2008) and GLFT (2013):
//!
//! **Reservation Price** (where you'd be indifferent to trading):
//! r = mid − γ × q × σ² × τ
//!
//! Where:
//! - γ = risk aversion parameter
//! - q = current inventory (positive = long)
//! - σ = volatility
//! - τ = time remaining in session
//!
//! **Optimal Half-Spread**:
//! δ = γσ²τ + (2/γ) × ln(1 + γ/k)
//!
//! Components:
//! - Volatility term (γσ²τ): Wider spreads in volatile markets
//! - Adverse selection term: Compensation for informed traders

use super::protocol::QuotingModel;
use crate::domain::{Inventory, Quote};
use chrono::Duration;
use trading_core::{Price, Quantity};

/// Avellaneda-Stoikov configuration
#[derive(Debug, Clone, Copy)]
pub struct ASConfig {
    /// Risk aversion parameter γ
    /// Higher = more conservative quotes
    pub gamma: f64,
    /// Intensity parameter k (order arrival rate)
    pub k: f64,
    /// Base quote size
    pub base_size: i64,
    /// Minimum half-spread (prevents too tight quotes)
    pub min_half_spread_bps: f64,
    /// Maximum half-spread (prevents too wide quotes)
    pub max_half_spread_bps: f64,
}

impl Default for ASConfig {
    fn default() -> Self {
        Self {
            gamma: 0.1,
            k: 1.5,
            base_size: 100_00000000, // 100 units
            min_half_spread_bps: 1.0,
            max_half_spread_bps: 100.0,
        }
    }
}

/// Avellaneda-Stoikov optimal market making model
pub struct AvellanedaStoikov {
    config: ASConfig,
}

impl AvellanedaStoikov {
    pub fn new(config: ASConfig) -> Self {
        Self { config }
    }

    /// Create with custom risk aversion
    pub fn with_gamma(gamma: f64) -> Self {
        Self::new(ASConfig {
            gamma: gamma.max(0.001),
            ..Default::default()
        })
    }

    /// Create conservative (risk-averse) market maker
    pub fn conservative() -> Self {
        Self::with_gamma(0.5)
    }

    /// Create aggressive market maker (tighter quotes)
    pub fn aggressive() -> Self {
        Self::with_gamma(0.02)
    }

    /// Calculate reservation price
    ///
    /// r = mid - γ × q × σ² × τ
    fn reservation_price(
        &self,
        mid_price: Price,
        inventory: &Inventory,
        volatility: f64,
        tau: f64,
    ) -> Price {
        let gamma = self.config.gamma;
        let q = inventory.ratio(); // Normalized position
        let sigma_sq = volatility * volatility;

        // Adjustment in price units
        let adjustment = gamma * q * sigma_sq * tau;

        // Convert to raw price adjustment
        let adjustment_raw = (mid_price.raw() as f64 * adjustment) as i64;

        Price::from_raw(mid_price.raw() - adjustment_raw)
    }

    /// Calculate optimal half-spread
    ///
    /// Simplified A-S model for practical use:
    /// δ = base_spread + vol_spread
    ///
    /// Where:
    /// - base_spread: minimum spread from adverse selection (scaled by gamma/k)
    /// - vol_spread: volatility-time component γσ²τ (scaled to bps)
    ///
    /// The full A-S formula δ = γσ²τ + (2/γ)ln(1 + γ/k) requires careful calibration.
    fn optimal_half_spread(&self, volatility: f64, tau: f64) -> f64 {
        let gamma = self.config.gamma;
        let k = self.config.k;
        let sigma_sq = volatility * volatility;

        // Base spread: higher gamma (risk aversion) and lower k (less liquidity) = wider
        // Scaled so typical values give reasonable bps
        let adverse_term = gamma * (1.0 + 1.0 / k).ln() * 100.0;

        // Volatility-time term: γσ²τ scaled to produce ~10-50 bps for typical values
        // tau is annualized, so 1 hour = 1/(365*24) ≈ 0.000114
        // We scale by 1_000_000 to get reasonable bps
        let vol_term = gamma * sigma_sq * tau * 1_000_000.0;

        let half_spread_bps = adverse_term + vol_term;

        // Clamp to configured range
        half_spread_bps.clamp(
            self.config.min_half_spread_bps,
            self.config.max_half_spread_bps,
        )
    }

    /// Convert tau from duration to annual fraction
    fn tau_from_duration(time_remaining: Duration) -> f64 {
        let seconds = time_remaining.num_seconds() as f64;
        let year_seconds = 365.25 * 24.0 * 3600.0;
        (seconds / year_seconds).max(0.0001) // Floor at small value
    }
}

impl QuotingModel for AvellanedaStoikov {
    fn compute_quotes(
        &self,
        mid_price: Price,
        inventory: &Inventory,
        volatility: f64,
        time_remaining: Duration,
    ) -> Quote {
        let tau = Self::tau_from_duration(time_remaining);
        let vol = volatility.max(0.01);

        // Calculate reservation price (skews quotes based on inventory)
        let reservation = self.reservation_price(mid_price, inventory, vol, tau);

        // Calculate optimal half-spread
        let half_spread_bps = self.optimal_half_spread(vol, tau);
        let half_spread_raw = (reservation.raw() as f64 * half_spread_bps / 10_000.0) as i64;

        // Compute bid and ask
        let bid_price = Price::from_raw(reservation.raw() - half_spread_raw);
        let ask_price = Price::from_raw(reservation.raw() + half_spread_raw);

        // Size based on inventory
        let (bid_size, ask_size) = if inventory.is_long() {
            // Long inventory: reduce bid size, increase ask
            let ratio = 1.0 - inventory.abs_ratio() * 0.5;
            (
                Quantity::from_raw((self.config.base_size as f64 * ratio) as i64),
                Quantity::from_raw(self.config.base_size),
            )
        } else if inventory.is_short() {
            // Short inventory: increase bid size, reduce ask
            let ratio = 1.0 - inventory.abs_ratio() * 0.5;
            (
                Quantity::from_raw(self.config.base_size),
                Quantity::from_raw((self.config.base_size as f64 * ratio) as i64),
            )
        } else {
            // Flat: symmetric
            (
                Quantity::from_raw(self.config.base_size),
                Quantity::from_raw(self.config.base_size),
            )
        };

        Quote::new(bid_price, ask_price, bid_size, ask_size)
    }

    fn name(&self) -> &str {
        "avellaneda_stoikov"
    }
}

impl Default for AvellanedaStoikov {
    fn default() -> Self {
        Self::new(ASConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symmetric_when_flat() {
        let model = AvellanedaStoikov::default();
        let inventory = Inventory::new(1000);

        let quote = model.compute_quotes(Price::from_int(100), &inventory, 0.2, Duration::hours(1));

        // Should be roughly symmetric around mid
        let mid_raw = quote.mid_price().raw();
        assert!((mid_raw - 100_00000000).abs() < 5_000_000); // Within 0.05
        assert!(!quote.is_crossed());
    }

    #[test]
    fn test_skew_when_long() {
        let model = AvellanedaStoikov::default();
        let long_inventory = Inventory::with_position(500, 1000);

        let quote = model.compute_quotes(
            Price::from_int(100),
            &long_inventory,
            0.2,
            Duration::hours(1),
        );

        // When long, reservation price should be below mid
        // So quotes should be shifted down to encourage selling
        assert!(quote.mid_price().raw() < 100_00000000);
        // Ask size should be >= bid size (encourage selling)
        assert!(quote.ask_size >= quote.bid_size);
    }

    #[test]
    fn test_skew_when_short() {
        let model = AvellanedaStoikov::default();
        let short_inventory = Inventory::with_position(-500, 1000);

        let quote = model.compute_quotes(
            Price::from_int(100),
            &short_inventory,
            0.2,
            Duration::hours(1),
        );

        // When short, reservation price should be above mid
        assert!(quote.mid_price().raw() > 100_00000000);
        // Bid size should be >= ask size (encourage buying)
        assert!(quote.bid_size >= quote.ask_size);
    }

    #[test]
    fn test_wider_spread_with_higher_volatility() {
        let model = AvellanedaStoikov::default();
        let inventory = Inventory::new(1000);

        let low_vol =
            model.compute_quotes(Price::from_int(100), &inventory, 0.1, Duration::hours(1));

        let high_vol =
            model.compute_quotes(Price::from_int(100), &inventory, 0.5, Duration::hours(1));

        assert!(high_vol.spread().raw() > low_vol.spread().raw());
    }

    #[test]
    fn test_wider_spread_with_more_time() {
        let model = AvellanedaStoikov::default();
        let inventory = Inventory::new(1000);

        let short_time =
            model.compute_quotes(Price::from_int(100), &inventory, 0.2, Duration::minutes(30));

        let long_time =
            model.compute_quotes(Price::from_int(100), &inventory, 0.2, Duration::hours(8));

        assert!(long_time.spread().raw() > short_time.spread().raw());
    }

    #[test]
    fn test_risk_aversion_affects_spread() {
        let conservative = AvellanedaStoikov::conservative();
        let aggressive = AvellanedaStoikov::aggressive();
        let inventory = Inventory::new(1000);

        let conservative_quote =
            conservative.compute_quotes(Price::from_int(100), &inventory, 0.2, Duration::hours(1));

        let aggressive_quote =
            aggressive.compute_quotes(Price::from_int(100), &inventory, 0.2, Duration::hours(1));

        assert!(conservative_quote.spread().raw() > aggressive_quote.spread().raw());
    }
}
