//! Mock Reference Feed
//!
//! A deterministic or random reference feed for testing and simulation.
//! Simulates Binance-like market data without network dependency.

use parking_lot::RwLock;
use rand::prelude::*;
use rand_distr::Normal;
use risk_management::{OrderbookMoments, ReferenceFeed};
use trading_core::Price;

/// Pre-defined market scenarios for simulation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Normal market conditions
    Normal,
    /// High volatility
    Volatile,
    /// Stressed conditions (wide spread, low depth)
    Stressed,
    /// Trending market (high imbalance)
    Trending,
    /// Crisis (extreme stress)
    Crisis,
}

impl Default for Scenario {
    fn default() -> Self {
        Self::Normal
    }
}

/// Configuration for the mock feed
#[derive(Debug, Clone)]
pub struct MockFeedConfig {
    /// Initial price (default: 50000)
    pub initial_price: i64,
    /// Scenario to simulate
    pub scenario: Scenario,
    /// Whether to add random noise
    pub add_noise: bool,
    /// Random seed (for reproducibility)
    pub seed: Option<u64>,
}

impl Default for MockFeedConfig {
    fn default() -> Self {
        Self {
            initial_price: 50000_00000000, // $50,000 with 8 decimals
            scenario: Scenario::Normal,
            add_noise: true,
            seed: None,
        }
    }
}

impl MockFeedConfig {
    pub fn with_scenario(mut self, scenario: Scenario) -> Self {
        self.scenario = scenario;
        self
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    pub fn deterministic(seed: u64) -> Self {
        Self {
            seed: Some(seed),
            add_noise: true,
            ..Default::default()
        }
    }
}

/// Mock reference feed for simulation
pub struct MockFeed {
    config: MockFeedConfig,
    state: RwLock<MockFeedState>,
}

struct MockFeedState {
    current_price: i64,
    moments: OrderbookMoments,
    tick_count: u64,
    rng: StdRng,
}

impl MockFeed {
    /// Create a new mock feed with default configuration
    pub fn new() -> Self {
        Self::with_config(MockFeedConfig::default())
    }

    /// Create with specific configuration
    pub fn with_config(config: MockFeedConfig) -> Self {
        let rng = match config.seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::from_entropy(),
        };

        let moments = Self::moments_for_scenario(config.scenario);

        Self {
            state: RwLock::new(MockFeedState {
                current_price: config.initial_price,
                moments,
                tick_count: 0,
                rng,
            }),
            config,
        }
    }

    /// Create a simple static feed (no noise, fixed values)
    pub fn static_feed(price: Price, moments: OrderbookMoments) -> Self {
        Self {
            config: MockFeedConfig {
                initial_price: price.raw(),
                scenario: Scenario::Normal,
                add_noise: false,
                seed: Some(0),
            },
            state: RwLock::new(MockFeedState {
                current_price: price.raw(),
                moments,
                tick_count: 0,
                rng: StdRng::seed_from_u64(0),
            }),
        }
    }

    /// Advance the feed by one tick
    pub fn tick(&self) {
        let mut state = self.state.write();
        state.tick_count += 1;

        if self.config.add_noise {
            // Update price with random walk
            let volatility = state.moments.mid_volatility;
            let normal = Normal::new(0.0, volatility * 0.01).unwrap();
            let price_change = normal.sample(&mut state.rng);
            let new_price = state.current_price as f64 * (1.0 + price_change);
            state.current_price = new_price as i64;

            // Add noise to moments
            self.add_moment_noise(&mut state);
        }
    }

    /// Set the current scenario
    pub fn set_scenario(&self, scenario: Scenario) {
        let mut state = self.state.write();
        state.moments = Self::moments_for_scenario(scenario);
    }

    /// Set specific moments (override scenario)
    pub fn set_moments(&self, moments: OrderbookMoments) {
        let mut state = self.state.write();
        state.moments = moments;
    }

    /// Set the current price
    pub fn set_price(&self, price: Price) {
        let mut state = self.state.write();
        state.current_price = price.raw();
    }

    /// Get the tick count
    pub fn tick_count(&self) -> u64 {
        self.state.read().tick_count
    }

    fn moments_for_scenario(scenario: Scenario) -> OrderbookMoments {
        match scenario {
            Scenario::Normal => OrderbookMoments {
                spread_bps: 10.0,
                depth_ratio: 0.8,
                imbalance: 0.0,
                mid_volatility: 0.02,
                ..Default::default()
            },
            Scenario::Volatile => OrderbookMoments {
                spread_bps: 30.0,
                depth_ratio: 0.6,
                imbalance: 0.1,
                mid_volatility: 0.08,
                ..Default::default()
            },
            Scenario::Stressed => OrderbookMoments {
                spread_bps: 80.0,
                depth_ratio: 0.2,
                imbalance: 0.3,
                mid_volatility: 0.05,
                ..Default::default()
            },
            Scenario::Trending => OrderbookMoments {
                spread_bps: 15.0,
                depth_ratio: 0.5,
                imbalance: 0.6, // Strong imbalance
                mid_volatility: 0.03,
                ..Default::default()
            },
            Scenario::Crisis => OrderbookMoments {
                spread_bps: 200.0,
                depth_ratio: 0.05,
                imbalance: 0.8,
                mid_volatility: 0.15,
                ..Default::default()
            },
        }
    }

    fn add_moment_noise(&self, state: &mut MockFeedState) {
        let base = Self::moments_for_scenario(self.config.scenario);

        // Add small random variations
        let spread_noise = Normal::new(0.0, base.spread_bps * 0.1).unwrap();
        let depth_noise = Normal::new(0.0, 0.05).unwrap();
        let imb_noise = Normal::new(0.0, 0.1).unwrap();
        let vol_noise = Normal::new(0.0, base.mid_volatility * 0.1).unwrap();

        state.moments.spread_bps = (base.spread_bps + spread_noise.sample(&mut state.rng)).max(1.0);
        state.moments.depth_ratio =
            (base.depth_ratio + depth_noise.sample(&mut state.rng)).clamp(0.01, 1.0);
        state.moments.imbalance =
            (base.imbalance + imb_noise.sample(&mut state.rng)).clamp(-1.0, 1.0);
        state.moments.mid_volatility =
            (base.mid_volatility + vol_noise.sample(&mut state.rng)).max(0.001);
    }
}

impl Default for MockFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl ReferenceFeed for MockFeed {
    fn moments(&self) -> OrderbookMoments {
        self.state.read().moments.clone()
    }

    fn mid_price(&self) -> Price {
        Price::from_raw(self.state.read().current_price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_feed() {
        let feed = MockFeed::new();

        assert_eq!(feed.mid_price(), Price::from_raw(50000_00000000));
        assert!(!feed.is_stressed());
    }

    #[test]
    fn test_scenario_moments() {
        let stressed_feed =
            MockFeed::with_config(MockFeedConfig::default().with_scenario(Scenario::Stressed));

        assert!(stressed_feed.is_stressed());
        assert!(stressed_feed.spread_bps() > 50.0);
    }

    #[test]
    fn test_deterministic() {
        let feed1 = MockFeed::with_config(MockFeedConfig::deterministic(42));
        let feed2 = MockFeed::with_config(MockFeedConfig::deterministic(42));

        // Advance both feeds
        for _ in 0..10 {
            feed1.tick();
            feed2.tick();
        }

        // Should have same state
        assert_eq!(feed1.mid_price(), feed2.mid_price());
        assert!((feed1.moments().spread_bps - feed2.moments().spread_bps).abs() < 0.001);
    }

    #[test]
    fn test_set_scenario() {
        let feed = MockFeed::new();

        assert!(!feed.is_stressed());

        feed.set_scenario(Scenario::Crisis);
        assert!(feed.is_stressed());
        assert!(feed.spread_bps() > 100.0);
    }

    #[test]
    fn test_static_feed() {
        let moments = OrderbookMoments {
            spread_bps: 25.0,
            depth_ratio: 0.5,
            imbalance: 0.2,
            mid_volatility: 0.04,
            ..Default::default()
        };

        let feed = MockFeed::static_feed(Price::from_int(100), moments.clone());

        // Tick shouldn't change anything
        for _ in 0..100 {
            feed.tick();
        }

        assert_eq!(feed.mid_price(), Price::from_int(100));
        assert!((feed.moments().spread_bps - 25.0).abs() < 0.001);
    }
}
