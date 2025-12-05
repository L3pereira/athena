//! Event Feed - External information source for informed trading
//!
//! Generates simulated market events that strategies can use:
//! - Fair value updates (from index prices, models, etc.)
//! - Volatility changes
//! - Sentiment signals (news, social media, etc.)
//!
//! Uses MarketEvent from the strategy crate so strategies can directly
//! consume events.

use athena_strategy::MarketEvent;
use rand::Rng;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use tokio::sync::broadcast;

/// Configuration for event feed simulation
#[derive(Debug, Clone)]
pub struct EventFeedConfig {
    /// Base fair values per instrument
    pub initial_fair_values: HashMap<String, Decimal>,
    /// Volatility for random walk (as percentage, e.g., 0.001 = 0.1%)
    pub price_volatility: Decimal,
    /// Probability of sentiment event per tick (0.0 to 1.0)
    pub sentiment_probability: f64,
    /// Max sentiment magnitude
    pub max_sentiment: Decimal,
}

impl Default for EventFeedConfig {
    fn default() -> Self {
        let mut initial_fair_values = HashMap::new();
        initial_fair_values.insert("BTC-USD".to_string(), dec!(50000));
        initial_fair_values.insert("ETH-USD".to_string(), dec!(3000));

        Self {
            initial_fair_values,
            price_volatility: dec!(0.0005), // 0.05% per tick
            sentiment_probability: 0.05,    // 5% chance per tick
            max_sentiment: dec!(0.5),
        }
    }
}

/// Generates simulated market events
pub struct EventFeedSimulator {
    /// Current fair values per instrument
    fair_values: HashMap<String, Decimal>,
    /// Configuration
    config: EventFeedConfig,
    /// Event broadcaster
    event_tx: broadcast::Sender<MarketEvent>,
    /// Random generator seed for reproducibility
    rng: rand::rngs::StdRng,
}

impl EventFeedSimulator {
    /// Create a new event feed simulator
    pub fn new(config: EventFeedConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let fair_values = config.initial_fair_values.clone();

        Self {
            fair_values,
            config,
            event_tx,
            rng: rand::SeedableRng::from_entropy(),
        }
    }

    /// Create with a specific seed for reproducible simulations
    pub fn with_seed(config: EventFeedConfig, seed: u64) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let fair_values = config.initial_fair_values.clone();

        Self {
            fair_values,
            config,
            event_tx,
            rng: rand::SeedableRng::seed_from_u64(seed),
        }
    }

    /// Subscribe to event feed
    pub fn subscribe(&self) -> broadcast::Receiver<MarketEvent> {
        self.event_tx.subscribe()
    }

    /// Get current fair value for an instrument
    pub fn fair_value(&self, instrument_id: &str) -> Option<Decimal> {
        self.fair_values.get(instrument_id).copied()
    }

    /// Generate next event (call this on each tick)
    pub fn next_event(&mut self) -> MarketEvent {
        // Pick a random instrument
        let instruments: Vec<_> = self.fair_values.keys().cloned().collect();
        let instrument_id = &instruments[self.rng.gen_range(0..instruments.len())];

        // Decide what type of event to generate
        let event_type: f64 = self.rng.r#gen();

        if event_type < self.config.sentiment_probability {
            // Generate sentiment event
            let score_f64: f64 = self.rng.gen_range(-1.0..1.0);
            let score = Decimal::from_f64_retain(
                score_f64
                    * self
                        .config
                        .max_sentiment
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.5),
            )
            .unwrap_or(dec!(0));

            MarketEvent::sentiment(instrument_id, score)
        } else {
            // Generate fair value update (random walk)
            let current = self
                .fair_values
                .get(instrument_id)
                .copied()
                .unwrap_or(dec!(50000));

            // Random walk: price * (1 + volatility * normal_random)
            let change_pct: f64 = self.rng.gen_range(-1.0..1.0);
            let vol_f64 = self
                .config
                .price_volatility
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0005);
            let multiplier = 1.0 + vol_f64 * change_pct;
            let new_price = Decimal::from_f64_retain(
                current.to_string().parse::<f64>().unwrap_or(50000.0) * multiplier,
            )
            .unwrap_or(current);

            self.fair_values.insert(instrument_id.clone(), new_price);

            MarketEvent::fair_value(instrument_id, new_price)
        }
    }

    /// Generate and broadcast next event
    pub fn tick(&mut self) -> MarketEvent {
        let event = self.next_event();
        // Ignore send error (no subscribers is ok)
        let _ = self.event_tx.send(event.clone());
        event
    }

    /// Run the event feed for a specified number of ticks
    pub async fn run_ticks(&mut self, num_ticks: usize, interval_ms: u64) {
        for _ in 0..num_ticks {
            self.tick();
            tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;
        }
    }

    /// Run continuously until cancelled
    pub async fn run(&mut self, interval_ms: u64) {
        loop {
            self.tick();
            tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_feed_generates_events() {
        let config = EventFeedConfig::default();
        let mut feed = EventFeedSimulator::with_seed(config, 42);

        // Generate some events
        for _ in 0..10 {
            let event = feed.next_event();
            match &event {
                MarketEvent::FairValue { price, .. } => {
                    assert!(price > &dec!(0));
                }
                MarketEvent::Sentiment { score, .. } => {
                    assert!(score >= &dec!(-1) && score <= &dec!(1));
                }
                MarketEvent::VolatilityChange { .. } => {}
            }
        }
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_fair_value_random_walk() {
        let mut config = EventFeedConfig::default();
        config.sentiment_probability = 0.0; // Only fair value events
        config.initial_fair_values.clear();
        config
            .initial_fair_values
            .insert("TEST".to_string(), dec!(100));

        let mut feed = EventFeedSimulator::with_seed(config, 42);

        let initial = feed.fair_value("TEST").unwrap();

        // Generate many events
        for _ in 0..100 {
            feed.next_event();
        }

        let final_val = feed.fair_value("TEST").unwrap();

        // Price should have changed but stay in reasonable range
        assert!(final_val > dec!(50) && final_val < dec!(200));
        println!("Initial: {}, Final: {}", initial, final_val);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config = EventFeedConfig::default();
        let mut feed = EventFeedSimulator::new(config);

        let mut rx = feed.subscribe();

        // Generate event
        let sent_event = feed.tick();

        // Should receive it
        let received = rx.try_recv().unwrap();

        // Check they match
        assert_eq!(sent_event.instrument_id(), received.instrument_id());
    }
}
