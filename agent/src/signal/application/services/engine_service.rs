//! Signal Engine Service - Use case for signal generation orchestration
//!
//! This service coordinates signal generation without depending on
//! concrete implementations. Uses dependency injection through ports.

use crate::signal::application::ports::{MarketDataPort, SignalGeneratorPort, SignalPublisher};
use crate::signal::domain::Signal;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, error, info};

/// Configuration for the signal engine service
#[derive(Debug, Clone)]
pub struct EngineServiceConfig {
    /// Tick interval in milliseconds
    pub tick_interval_ms: u64,
    /// Whether to log statistics periodically
    pub log_stats: bool,
    /// Stats logging interval (number of ticks)
    pub stats_log_interval: u64,
}

impl Default for EngineServiceConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: 10,
            log_stats: true,
            stats_log_interval: 10000,
        }
    }
}

/// Signal generation engine use case
///
/// Orchestrates the signal generation loop:
/// 1. Read market data through MarketDataPort
/// 2. Call strategy through SignalGeneratorPort
/// 3. Publish signals through SignalPublisher
///
/// This separation allows:
/// - Testing with mock implementations
/// - Different market data sources
/// - Different signal output mechanisms
pub struct EngineService;

impl EngineService {
    /// Run a signal generator in a loop
    ///
    /// This is the core loop that:
    /// - Ticks at the configured interval
    /// - Calls the strategy's on_tick method
    /// - Publishes generated signals
    /// - Logs periodic statistics
    pub fn run_generator<G, M, P>(
        mut generator: G,
        market_data: Arc<M>,
        publisher: P,
        config: EngineServiceConfig,
        shutdown: Arc<AtomicBool>,
    ) where
        G: SignalGeneratorPort,
        M: MarketDataPort,
        P: SignalPublisher,
    {
        let strategy_name = generator.name().to_string();
        let tick_interval = Duration::from_millis(config.tick_interval_ms);

        info!(
            "Starting signal generator '{}' with {}ms tick interval",
            strategy_name, config.tick_interval_ms
        );

        generator.on_start();

        let mut last_stats_time = Instant::now();
        let mut tick_count: u64 = 0;
        let mut signal_count: u64 = 0;

        while !shutdown.load(Ordering::Relaxed) {
            let tick_start = Instant::now();

            // Generate signals
            let signals = generator.on_tick(market_data.as_ref());
            tick_count += 1;

            // Publish signals
            for signal in signals {
                if let Err(e) = publisher.publish(signal) {
                    error!("Failed to publish signal from '{}': {}", strategy_name, e);
                    // Continue - don't break the loop on publish failure
                }
                signal_count += 1;
            }

            // Log statistics periodically
            if config.log_stats && tick_count.is_multiple_of(config.stats_log_interval) {
                let elapsed = last_stats_time.elapsed();
                let tps = config.stats_log_interval as f64 / elapsed.as_secs_f64();
                debug!(
                    "Generator '{}': {:.0} ticks/s, {} total signals",
                    strategy_name, tps, signal_count
                );
                last_stats_time = Instant::now();
            }

            // Maintain tick rate
            let elapsed = tick_start.elapsed();
            if elapsed < tick_interval {
                std::thread::sleep(tick_interval - elapsed);
            }
        }

        generator.on_stop();
        info!(
            "Signal generator '{}' stopped after {} ticks, {} signals",
            strategy_name, tick_count, signal_count
        );
    }

    /// Process a single tick (useful for testing)
    pub fn tick<G, M, P>(generator: &mut G, market_data: &M, publisher: &P) -> Vec<Signal>
    where
        G: SignalGeneratorPort,
        M: MarketDataPort,
        P: SignalPublisher,
    {
        let signals = generator.on_tick(market_data);

        for signal in &signals {
            if let Err(e) = publisher.publish(signal.clone()) {
                error!("Failed to publish signal: {}", e);
            }
        }

        signals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::application::ports::{
        BookLevel, GeneratorConfig, OrderBookReader, PublishError, SymbolKey,
    };
    use crate::signal::domain::{Signal, SignalDirection, StrategyId, StrategyType};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use trading_core::{Price, Quantity};

    // Mock implementations for testing

    struct MockOrderBook {
        mid: Price,
    }

    impl OrderBookReader for MockOrderBook {
        fn is_initialized(&self) -> bool {
            true
        }
        fn best_bid(&self) -> Option<BookLevel> {
            Some(BookLevel {
                price: Price::from_raw(self.mid.raw() - Price::from_int(1).raw()),
                size: Quantity::from_int(10),
            })
        }
        fn best_ask(&self) -> Option<BookLevel> {
            Some(BookLevel {
                price: Price::from_raw(self.mid.raw() + Price::from_int(1).raw()),
                size: Quantity::from_int(10),
            })
        }
        fn mid_price(&self) -> Option<Price> {
            Some(self.mid)
        }
        fn spread(&self) -> Option<Price> {
            Some(Price::from_int(2))
        }
        fn bid_levels(&self, _: usize) -> Vec<BookLevel> {
            vec![]
        }
        fn ask_levels(&self, _: usize) -> Vec<BookLevel> {
            vec![]
        }
        fn total_bid_depth(&self, _: usize) -> Quantity {
            Quantity::from_int(10)
        }
        fn total_ask_depth(&self, _: usize) -> Quantity {
            Quantity::from_int(10)
        }
        fn last_update_time(&self) -> Option<u64> {
            None
        }
    }

    struct MockMarketData {
        books: HashMap<String, Arc<MockOrderBook>>,
    }

    impl MarketDataPort for MockMarketData {
        type BookReader = MockOrderBook;

        fn book(&self, key: &SymbolKey) -> Arc<MockOrderBook> {
            self.books
                .get(&key.to_string())
                .cloned()
                .unwrap_or_else(|| Arc::new(MockOrderBook { mid: Price::ZERO }))
        }

        fn has_symbol(&self, key: &SymbolKey) -> bool {
            self.books.contains_key(&key.to_string())
        }

        fn symbols(&self) -> Vec<SymbolKey> {
            vec![]
        }
    }

    struct MockPublisher {
        signals: Arc<Mutex<Vec<Signal>>>,
    }

    impl SignalPublisher for MockPublisher {
        fn publish(&self, signal: Signal) -> Result<(), PublishError> {
            self.signals.lock().unwrap().push(signal);
            Ok(())
        }
    }

    struct MockGenerator {
        config: GeneratorConfig,
        tick_count: u64,
    }

    impl SignalGeneratorPort for MockGenerator {
        fn config(&self) -> &GeneratorConfig {
            &self.config
        }

        fn on_tick<M: MarketDataPort>(&mut self, market_data: &M) -> Vec<Signal> {
            self.tick_count += 1;

            let key = &self.config.symbols[0];
            let book = market_data.book(key);

            if book.is_initialized()
                && let Some(mid) = book.mid_price()
            {
                return vec![
                    Signal::builder(
                        self.config.strategy_id.clone(),
                        self.config.strategy_type,
                        key.to_string(),
                    )
                    .direction(SignalDirection::Buy)
                    .strength(0.5)
                    .confidence(0.8)
                    .prices(mid, mid)
                    .build(),
                ];
            }

            vec![]
        }
    }

    #[test]
    fn test_single_tick() {
        let mut market_data = MockMarketData {
            books: HashMap::new(),
        };
        let key = SymbolKey::new("test", "BTCUSDT");
        market_data.books.insert(
            key.to_string(),
            Arc::new(MockOrderBook {
                mid: Price::from_int(100),
            }),
        );

        let signals = Arc::new(Mutex::new(Vec::new()));
        let publisher = MockPublisher {
            signals: signals.clone(),
        };

        let mut generator = MockGenerator {
            config: GeneratorConfig::new(
                StrategyId::new("test"),
                StrategyType::MeanReversion,
                vec![key],
            ),
            tick_count: 0,
        };

        let result = EngineService::tick(&mut generator, &market_data, &publisher);

        assert_eq!(result.len(), 1);
        assert_eq!(signals.lock().unwrap().len(), 1);
    }
}
