//! Statistical moments describing orderbook structure
//!
//! These moments characterize the shape of an orderbook without specifying
//! individual orders. Used by the synthetic orderbook generator.

use serde::{Deserialize, Serialize};

/// Number of price levels to generate on each side
pub const NUM_LEVELS: usize = 10;

/// Statistical description of orderbook structure
///
/// Captures the key moments needed to generate synthetic orderbooks:
/// - Spread distribution (log-normal)
/// - Depth per level (log-normal with exponential decay)
/// - Imbalance between bid/ask sides
/// - Correlation structure between levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookMoments {
    // Spread distribution (log-normal in bps)
    /// Mean spread in basis points
    pub spread_mean_bps: f64,
    /// Variance of spread in basis points
    pub spread_var_bps: f64,

    // Depth per level (log-normal)
    /// Mean depth at each level (0 = best, decays away from mid)
    pub depth_mean: [f64; NUM_LEVELS],
    /// Variance of depth at each level
    pub depth_var: [f64; NUM_LEVELS],

    // Imbalance (truncated normal, range [-1, 1])
    /// Mean bid/ask imbalance (-1 = all asks, 0 = balanced, 1 = all bids)
    pub imbalance_mean: f64,
    /// Variance of imbalance
    pub imbalance_var: f64,

    // Shape parameters
    /// Exponential decay rate for depth across levels
    pub decay_rate: f64,
    /// Correlation between adjacent price levels (for copula)
    pub level_correlation: f64,
    /// Tick size in basis points of mid price
    pub tick_size_bps: f64,
}

impl OrderbookMoments {
    /// Create moments for a normal/calm market regime
    ///
    /// Characteristics:
    /// - Tight spreads (~5 bps)
    /// - Deep book
    /// - Balanced imbalance
    /// - Moderate correlation between levels
    pub fn default_normal() -> Self {
        Self {
            spread_mean_bps: 5.0,
            spread_var_bps: 4.0,
            depth_mean: Self::compute_depth_mean(500.0, 0.15),
            depth_var: Self::compute_depth_var(500.0, 0.15, 0.3),
            imbalance_mean: 0.0,
            imbalance_var: 0.0001,
            decay_rate: 0.15,
            level_correlation: 0.6,
            tick_size_bps: 1.0,
        }
    }

    /// Create moments for a volatile market regime
    ///
    /// Characteristics:
    /// - Wide spreads (~20 bps)
    /// - Thin book
    /// - Slightly more imbalanced
    /// - Lower correlation (more chaotic)
    pub fn default_volatile() -> Self {
        Self {
            spread_mean_bps: 20.0,
            spread_var_bps: 64.0,
            depth_mean: Self::compute_depth_mean(150.0, 0.25),
            depth_var: Self::compute_depth_var(150.0, 0.25, 0.5),
            imbalance_mean: 0.0,
            imbalance_var: 0.01,
            decay_rate: 0.25,
            level_correlation: 0.4,
            tick_size_bps: 2.0,
        }
    }

    /// Create moments for a trending market regime
    ///
    /// Characteristics:
    /// - Medium spreads (~10 bps)
    /// - Medium depth
    /// - Strong positive imbalance (bid-heavy, expecting up-move)
    /// - Higher correlation (orderly trend)
    pub fn default_trending() -> Self {
        Self {
            spread_mean_bps: 10.0,
            spread_var_bps: 16.0,
            depth_mean: Self::compute_depth_mean(350.0, 0.18),
            depth_var: Self::compute_depth_var(350.0, 0.18, 0.35),
            imbalance_mean: 0.35, // Strong directional pressure
            imbalance_var: 0.02,  // Some variance around the trend
            decay_rate: 0.18,
            level_correlation: 0.7,
            tick_size_bps: 1.0,
        }
    }

    /// Compute log-normal parameters for spread
    ///
    /// Given target mean and variance in real space, compute
    /// the mu and sigma for log-normal distribution.
    pub fn spread_lognormal_params(&self) -> (f64, f64) {
        lognormal_params(self.spread_mean_bps, self.spread_var_bps)
    }

    /// Compute log-normal parameters for depth at a given level
    pub fn depth_lognormal_params(&self, level: usize) -> (f64, f64) {
        let mean = self.depth_mean[level];
        let var = self.depth_var[level];
        lognormal_params(mean, var)
    }

    /// Helper to compute exponentially decaying depth means
    fn compute_depth_mean(base_depth: f64, decay: f64) -> [f64; NUM_LEVELS] {
        let mut depths = [0.0; NUM_LEVELS];
        for i in 0..NUM_LEVELS {
            depths[i] = base_depth * (-decay * i as f64).exp();
        }
        depths
    }

    /// Helper to compute exponentially decaying depth variances
    fn compute_depth_var(base_depth: f64, decay: f64, cv: f64) -> [f64; NUM_LEVELS] {
        let mut vars = [0.0; NUM_LEVELS];
        for i in 0..NUM_LEVELS {
            let mean = base_depth * (-decay * i as f64).exp();
            // Coefficient of variation stays roughly constant
            vars[i] = (cv * mean).powi(2);
        }
        vars
    }
}

/// Compute log-normal distribution parameters from target mean and variance
///
/// For X ~ LogNormal(mu, sigma):
///   E[X] = exp(mu + sigma^2/2)
///   Var[X] = (exp(sigma^2) - 1) * exp(2*mu + sigma^2)
///
/// Solving for mu and sigma given mean and variance:
///   sigma^2 = ln(1 + var/mean^2)
///   mu = ln(mean) - sigma^2/2
fn lognormal_params(mean: f64, var: f64) -> (f64, f64) {
    let sigma_sq = (1.0 + var / (mean * mean)).ln();
    let sigma = sigma_sq.sqrt();
    let mu = mean.ln() - sigma_sq / 2.0;
    (mu, sigma)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_regime() {
        let moments = OrderbookMoments::default_normal();
        assert!((moments.spread_mean_bps - 5.0).abs() < 0.001);
        assert!(moments.depth_mean[0] > moments.depth_mean[9]);
        assert!((moments.imbalance_mean - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_volatile_regime() {
        let moments = OrderbookMoments::default_volatile();
        assert!(moments.spread_mean_bps > 15.0);
        assert!(moments.depth_mean[0] < OrderbookMoments::default_normal().depth_mean[0]);
    }

    #[test]
    fn test_trending_regime() {
        let moments = OrderbookMoments::default_trending();
        assert!(moments.imbalance_mean > 0.0);
    }

    #[test]
    fn test_lognormal_params() {
        let mean = 100.0;
        let var = 400.0; // std = 20
        let (mu, sigma) = lognormal_params(mean, var);

        // Verify: exp(mu + sigma^2/2) should equal mean
        let computed_mean = (mu + sigma * sigma / 2.0).exp();
        assert!((computed_mean - mean).abs() < 0.01);
    }

    #[test]
    fn test_depth_decay() {
        let moments = OrderbookMoments::default_normal();

        // Each level should have less depth than previous
        for i in 1..NUM_LEVELS {
            assert!(moments.depth_mean[i] < moments.depth_mean[i - 1]);
        }

        // Decay should follow exponential pattern
        let ratio = moments.depth_mean[1] / moments.depth_mean[0];
        let expected_ratio = (-moments.decay_rate).exp();
        assert!((ratio - expected_ratio).abs() < 0.001);
    }
}
