//! Synthetic orderbook generator using Gaussian Copula
//!
//! Generates orderbooks that match target statistical moments.
//! Uses copula-based sampling to capture correlation between price levels.

use crate::domain::{NUM_LEVELS, OrderbookMoments};
use rand::prelude::*;
use rand_distr::{LogNormal, StandardNormal};
use trading_core::{Price, PriceLevel, Quantity};

/// Generated orderbook output
///
/// Contains the bid/ask levels ready to be used by the exchange.
/// All prices and quantities are in fixed-point i64 format.
#[derive(Debug, Clone)]
pub struct GeneratedOrderbook {
    /// Mid price used for generation
    pub mid_price: Price,
    /// Bid levels (best bid first, descending prices)
    pub bid_levels: Vec<PriceLevel>,
    /// Ask levels (best ask first, ascending prices)
    pub ask_levels: Vec<PriceLevel>,
    /// Actual spread after tick rounding
    pub spread: Price,
    /// Computed imbalance: (total_bid - total_ask) / (total_bid + total_ask)
    pub imbalance: f64,
}

impl GeneratedOrderbook {
    /// Get best bid price
    pub fn best_bid(&self) -> Price {
        self.bid_levels
            .first()
            .map(|l| l.price)
            .unwrap_or(Price::ZERO)
    }

    /// Get best ask price
    pub fn best_ask(&self) -> Price {
        self.ask_levels
            .first()
            .map(|l| l.price)
            .unwrap_or(Price::ZERO)
    }

    /// Total bid depth
    pub fn total_bid_depth(&self) -> Quantity {
        self.bid_levels
            .iter()
            .fold(Quantity::ZERO, |acc, l| acc + l.quantity)
    }

    /// Total ask depth
    pub fn total_ask_depth(&self) -> Quantity {
        self.ask_levels
            .iter()
            .fold(Quantity::ZERO, |acc, l| acc + l.quantity)
    }
}

/// Synthetic orderbook generator
///
/// Uses Gaussian Copula to generate correlated depths across levels,
/// then transforms to log-normal marginals.
pub struct SyntheticOrderbookGenerator {
    /// Current moments defining the orderbook shape
    moments: OrderbookMoments,
    /// Seeded random number generator for reproducibility
    rng: StdRng,
    /// Cholesky decomposition of correlation matrix (lower triangular)
    cholesky: [[f64; NUM_LEVELS]; NUM_LEVELS],
}

impl SyntheticOrderbookGenerator {
    /// Create a new generator with given moments and seed
    pub fn new(moments: OrderbookMoments, seed: u64) -> Self {
        let cholesky = Self::compute_cholesky(moments.level_correlation);
        Self {
            moments,
            rng: StdRng::seed_from_u64(seed),
            cholesky,
        }
    }

    /// Create generator from regime name
    ///
    /// # Arguments
    /// * `regime` - One of "normal", "volatile", "trending"
    /// * `seed` - Random seed for reproducibility
    pub fn from_regime(regime: &str, seed: u64) -> Self {
        let moments = match regime {
            "volatile" => OrderbookMoments::default_volatile(),
            "trending" => OrderbookMoments::default_trending(),
            _ => OrderbookMoments::default_normal(),
        };
        Self::new(moments, seed)
    }

    /// Update moments (for regime switching)
    pub fn update_moments(&mut self, moments: OrderbookMoments) {
        // Recompute Cholesky if correlation changed
        if (moments.level_correlation - self.moments.level_correlation).abs() > 1e-10 {
            self.cholesky = Self::compute_cholesky(moments.level_correlation);
        }
        self.moments = moments;
    }

    /// Get current moments
    pub fn moments(&self) -> &OrderbookMoments {
        &self.moments
    }

    /// Generate a synthetic orderbook at the given mid price
    pub fn generate(&mut self, mid_price: Price) -> GeneratedOrderbook {
        let mid_f64 = mid_price.to_f64();

        // 1. Sample spread from log-normal
        let spread_bps = self.sample_spread();
        let half_spread = mid_f64 * spread_bps / 20000.0; // bps to fraction, then half

        // 2. Compute tick size from moments
        let tick_size = mid_f64 * self.moments.tick_size_bps / 10000.0;
        let tick_price = Price::from_f64(tick_size);

        // 3. Sample target imbalance and generate scaled depths
        let target_imbalance = self.sample_imbalance();

        // 4. Generate correlated depths for each side, scaled by imbalance
        // imbalance = (bid - ask) / (bid + ask)
        // So: bid_scale = 1 + imbalance, ask_scale = 1 - imbalance (approximately)
        let bid_scale = 1.0 + target_imbalance;
        let ask_scale = 1.0 - target_imbalance;

        let bid_depths: [f64; NUM_LEVELS] = self.sample_depths().map(|d| d * bid_scale);
        let ask_depths: [f64; NUM_LEVELS] = self.sample_depths().map(|d| d * ask_scale);

        // 4. Build price levels
        let mut bid_levels = Vec::with_capacity(NUM_LEVELS);
        let mut ask_levels = Vec::with_capacity(NUM_LEVELS);

        // Best bid = mid - half_spread, rounded down to tick
        let best_bid_f64 = mid_f64 - half_spread;
        let best_bid = Price::from_f64(best_bid_f64).round_to_tick(tick_price);

        // Best ask = mid + half_spread, rounded up to tick
        let best_ask_f64 = mid_f64 + half_spread;
        let best_ask = Price::from_f64(best_ask_f64 + tick_size - 0.0001).round_to_tick(tick_price);

        // Build bid levels (prices decrease)
        for (i, &depth) in bid_depths.iter().enumerate() {
            let price = best_bid - tick_price * (i as i64);
            let quantity = Quantity::from_f64(depth);
            if quantity.raw() > 0 {
                bid_levels.push(PriceLevel::new(price, quantity));
            }
        }

        // Build ask levels (prices increase)
        for (i, &depth) in ask_depths.iter().enumerate() {
            let price = best_ask + tick_price * (i as i64);
            let quantity = Quantity::from_f64(depth);
            if quantity.raw() > 0 {
                ask_levels.push(PriceLevel::new(price, quantity));
            }
        }

        // Compute actual spread and imbalance
        let spread = best_ask - best_bid;
        let total_bid: f64 = bid_depths.iter().sum();
        let total_ask: f64 = ask_depths.iter().sum();
        let imbalance = if total_bid + total_ask > 0.0 {
            (total_bid - total_ask) / (total_bid + total_ask)
        } else {
            0.0
        };

        GeneratedOrderbook {
            mid_price,
            bid_levels,
            ask_levels,
            spread,
            imbalance,
        }
    }

    /// Sample spread from log-normal distribution
    fn sample_spread(&mut self) -> f64 {
        let (mu, sigma) = self.moments.spread_lognormal_params();
        let dist = LogNormal::new(mu, sigma).unwrap();
        self.rng.sample(dist)
    }

    /// Sample imbalance from truncated normal distribution
    ///
    /// Returns a value in [-1, 1] centered on imbalance_mean
    fn sample_imbalance(&mut self) -> f64 {
        let mean = self.moments.imbalance_mean;
        let std = self.moments.imbalance_var.sqrt();

        // Sample from normal and clamp to valid range
        let z: f64 = self.rng.sample(StandardNormal);
        (mean + std * z).clamp(-0.95, 0.95)
    }

    /// Sample correlated depths using Gaussian Copula
    ///
    /// Algorithm:
    /// 1. Generate independent standard normals
    /// 2. Apply Cholesky to induce correlation
    /// 3. Transform through normal CDF to get uniform
    /// 4. Transform through log-normal inverse CDF to get depths
    fn sample_depths(&mut self) -> [f64; NUM_LEVELS] {
        // Step 1: Independent standard normals
        let mut z = [0.0; NUM_LEVELS];
        for i in 0..NUM_LEVELS {
            z[i] = self.rng.sample(StandardNormal);
        }

        // Step 2: Apply Cholesky decomposition to correlate
        let mut correlated = [0.0; NUM_LEVELS];
        for i in 0..NUM_LEVELS {
            for j in 0..=i {
                correlated[i] += self.cholesky[i][j] * z[j];
            }
        }

        // Step 3 & 4: Transform to log-normal marginals
        // Φ(z) gives uniform, then apply log-normal quantile
        let mut depths = [0.0; NUM_LEVELS];
        for i in 0..NUM_LEVELS {
            let (mu, sigma) = self.moments.depth_lognormal_params(i);
            // Transform correlated normal to log-normal
            // For Z ~ N(0,1), exp(mu + sigma*Z) ~ LogNormal(mu, sigma)
            depths[i] = (mu + sigma * correlated[i]).exp();
        }

        depths
    }

    /// Compute Cholesky decomposition of AR(1) correlation matrix
    ///
    /// For correlation ρ, the matrix is:
    /// ```text
    /// [1    ρ    ρ²   ρ³  ...]
    /// [ρ    1    ρ    ρ²  ...]
    /// [ρ²   ρ    1    ρ   ...]
    /// ...
    /// ```
    ///
    /// This has a closed-form Cholesky decomposition.
    fn compute_cholesky(rho: f64) -> [[f64; NUM_LEVELS]; NUM_LEVELS] {
        let mut l = [[0.0; NUM_LEVELS]; NUM_LEVELS];

        // For AR(1) correlation matrix, Cholesky has special structure:
        // L[0][0] = 1
        // L[i][0] = ρ^i
        // L[i][i] = sqrt(1 - ρ^(2i))  for i > 0
        // L[i][j] = 0 for j > 0 and j < i

        // Actually, for simplicity use standard Cholesky algorithm:
        // This handles the general case correctly
        let mut corr = [[0.0; NUM_LEVELS]; NUM_LEVELS];
        for i in 0..NUM_LEVELS {
            for j in 0..NUM_LEVELS {
                let dist = (i as i32 - j as i32).abs() as f64;
                corr[i][j] = rho.powf(dist);
            }
        }

        // Standard Cholesky decomposition
        for i in 0..NUM_LEVELS {
            for j in 0..=i {
                let mut sum = corr[i][j];
                for k in 0..j {
                    sum -= l[i][k] * l[j][k];
                }
                if i == j {
                    l[i][j] = sum.sqrt();
                } else {
                    l[i][j] = sum / l[j][j];
                }
            }
        }

        l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_with_seed() {
        let mut generator1 = SyntheticOrderbookGenerator::from_regime("normal", 42);
        let mut generator2 = SyntheticOrderbookGenerator::from_regime("normal", 42);

        let mid = Price::from_f64(100.0);
        let book1 = generator1.generate(mid);
        let book2 = generator2.generate(mid);

        assert_eq!(book1.best_bid().raw(), book2.best_bid().raw());
        assert_eq!(book1.best_ask().raw(), book2.best_ask().raw());
        assert_eq!(book1.bid_levels.len(), book2.bid_levels.len());
    }

    #[test]
    fn test_spread_positive() {
        let mut generator = SyntheticOrderbookGenerator::from_regime("normal", 42);
        let mid = Price::from_f64(50000.0);

        for _ in 0..100 {
            let book = generator.generate(mid);
            assert!(book.spread.raw() > 0);
            assert!(book.best_ask().raw() > book.best_bid().raw());
        }
    }

    #[test]
    fn test_bid_ask_levels_ordered() {
        let mut generator = SyntheticOrderbookGenerator::from_regime("normal", 42);
        let mid = Price::from_f64(100.0);
        let book = generator.generate(mid);

        // Bids should be in descending order
        for i in 1..book.bid_levels.len() {
            assert!(book.bid_levels[i].price < book.bid_levels[i - 1].price);
        }

        // Asks should be in ascending order
        for i in 1..book.ask_levels.len() {
            assert!(book.ask_levels[i].price > book.ask_levels[i - 1].price);
        }
    }

    #[test]
    fn test_regime_affects_spread() {
        let mid = Price::from_f64(100.0);

        let mut normal_gen = SyntheticOrderbookGenerator::from_regime("normal", 42);
        let mut volatile_gen = SyntheticOrderbookGenerator::from_regime("volatile", 42);

        // Generate many and compare means
        let normal_spreads: Vec<f64> = (0..100)
            .map(|_| normal_gen.generate(mid).spread.to_f64())
            .collect();
        let volatile_spreads: Vec<f64> = (0..100)
            .map(|_| volatile_gen.generate(mid).spread.to_f64())
            .collect();

        let normal_mean: f64 = normal_spreads.iter().sum::<f64>() / 100.0;
        let volatile_mean: f64 = volatile_spreads.iter().sum::<f64>() / 100.0;

        // Volatile should have wider spreads on average
        assert!(volatile_mean > normal_mean * 2.0);
    }

    #[test]
    fn test_update_moments() {
        let mut generator = SyntheticOrderbookGenerator::from_regime("normal", 42);
        let mid = Price::from_f64(100.0);

        let book_normal = generator.generate(mid);

        generator.update_moments(OrderbookMoments::default_volatile());
        let book_volatile = generator.generate(mid);

        // After switching to volatile, spread should be wider
        // (statistically, not guaranteed for single sample)
        // Just verify it didn't crash
        assert!(book_normal.spread.raw() > 0);
        assert!(book_volatile.spread.raw() > 0);
    }

    #[test]
    fn test_cholesky_valid() {
        let rho = 0.6;
        let chol = SyntheticOrderbookGenerator::compute_cholesky(rho);

        // Verify L * L^T = correlation matrix
        for i in 0..NUM_LEVELS {
            for j in 0..NUM_LEVELS {
                let mut sum = 0.0;
                for k in 0..NUM_LEVELS {
                    sum += chol[i][k] * chol[j][k];
                }
                let expected = rho.powf((i as i32 - j as i32).abs() as f64);
                assert!(
                    (sum - expected).abs() < 1e-10,
                    "Cholesky verification failed at [{i}][{j}]"
                );
            }
        }
    }

    #[test]
    fn test_imbalance_in_range() {
        let mut generator = SyntheticOrderbookGenerator::from_regime("normal", 42);
        let mid = Price::from_f64(100.0);

        for _ in 0..100 {
            let book = generator.generate(mid);
            assert!(book.imbalance >= -1.0 && book.imbalance <= 1.0);
        }
    }
}
