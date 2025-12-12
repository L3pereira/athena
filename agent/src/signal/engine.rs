//! Signal Engine - Manages multiple strategies on dedicated OS threads
//!
//! The SignalEngine orchestrates signal generation by:
//! 1. Running each strategy on its own dedicated OS thread
//! 2. Providing lock-free access to order books via ArcSwap
//! 3. Aggregating signals from all strategies into a single output channel

use crate::order_book::OrderBookManager;
use crate::signal::traits::{SignalGenerator, SignalReceiver};
use crate::signal::types::Signal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Configuration for the SignalEngine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEngineConfig {
    /// Base tick interval in milliseconds
    pub tick_interval_ms: u64,
    /// Maximum signals per second (rate limiting)
    pub max_signals_per_second: Option<u32>,
    /// Signal channel buffer size
    pub channel_buffer_size: usize,
}

impl Default for SignalEngineConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: 10, // 100 Hz default
            max_signals_per_second: Some(1000),
            channel_buffer_size: 10000,
        }
    }
}

/// Handle to a running strategy thread
pub struct StrategyHandle {
    /// Strategy name
    pub name: String,
    /// Thread join handle
    handle: Option<JoinHandle<()>>,
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
}

impl StrategyHandle {
    /// Signal the strategy to shutdown
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Check if strategy is still running
    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }

    /// Wait for the strategy thread to finish
    pub fn join(mut self) -> thread::Result<()> {
        if let Some(handle) = self.handle.take() {
            handle.join()
        } else {
            Ok(())
        }
    }
}

/// Signal generation engine
///
/// Manages multiple strategies running on dedicated OS threads.
/// Each strategy reads order books (lock-free) and emits signals
/// to a centralized channel for downstream processing.
pub struct SignalEngine {
    config: SignalEngineConfig,
    book_manager: OrderBookManager,
    strategies: Vec<StrategyHandle>,
    signal_tx: mpsc::UnboundedSender<Signal>,
    signal_rx: Option<mpsc::UnboundedReceiver<Signal>>,
    shutdown: Arc<AtomicBool>,
}

impl SignalEngine {
    /// Create a new SignalEngine
    pub fn new(config: SignalEngineConfig, book_manager: OrderBookManager) -> Self {
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();

        Self {
            config,
            book_manager,
            strategies: Vec::new(),
            signal_tx,
            signal_rx: Some(signal_rx),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Add a strategy to the engine
    ///
    /// The strategy will run on its own dedicated OS thread.
    /// Call `start()` to begin signal generation.
    pub fn add_strategy<S: SignalGenerator>(&mut self, strategy: S) {
        let name = strategy.name().to_string();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        let book_manager = self.book_manager.clone();
        let signal_tx = self.signal_tx.clone();
        let tick_interval = Duration::from_millis(self.config.tick_interval_ms);

        let handle = thread::Builder::new()
            .name(format!("strategy-{}", name))
            .spawn(move || {
                Self::strategy_loop(
                    strategy,
                    book_manager,
                    signal_tx,
                    tick_interval,
                    shutdown_clone,
                );
            })
            .expect("Failed to spawn strategy thread");

        self.strategies.push(StrategyHandle {
            name,
            handle: Some(handle),
            shutdown,
        });
    }

    /// Strategy main loop running on dedicated thread
    fn strategy_loop<S: SignalGenerator>(
        mut strategy: S,
        book_manager: OrderBookManager,
        signal_tx: mpsc::UnboundedSender<Signal>,
        tick_interval: Duration,
        shutdown: Arc<AtomicBool>,
    ) {
        let strategy_name = strategy.name().to_string();
        info!(
            "Strategy {} started on thread {:?}",
            strategy_name,
            thread::current().id()
        );

        let mut last_tick = Instant::now();
        let mut signal_count: u64 = 0;
        let mut tick_count: u64 = 0;

        while !shutdown.load(Ordering::Relaxed) {
            let tick_start = Instant::now();

            // Generate signals
            let signals = strategy.on_tick(&book_manager);
            tick_count += 1;

            // Send signals
            for signal in signals {
                if let Err(e) = signal_tx.send(signal) {
                    error!("Strategy {} failed to send signal: {}", strategy_name, e);
                    break;
                }
                signal_count += 1;
            }

            // Log stats periodically
            if tick_count.is_multiple_of(10000) {
                let elapsed = last_tick.elapsed();
                let tps = 10000.0 / elapsed.as_secs_f64();
                debug!(
                    "Strategy {}: {} ticks/s, {} total signals",
                    strategy_name, tps as u64, signal_count
                );
                last_tick = Instant::now();
            }

            // Sleep to maintain tick rate
            let elapsed = tick_start.elapsed();
            if elapsed < tick_interval {
                thread::sleep(tick_interval - elapsed);
            }
        }

        info!(
            "Strategy {} shutting down after {} signals",
            strategy_name, signal_count
        );
    }

    /// Take the signal receiver
    ///
    /// Returns the receiver for consuming signals from all strategies.
    /// Can only be called once.
    pub fn take_signal_receiver(&mut self) -> Option<SignalReceiver> {
        self.signal_rx.take().map(SignalReceiver::new)
    }

    /// Get a clone of the signal sender (for testing)
    pub fn signal_sender(&self) -> mpsc::UnboundedSender<Signal> {
        self.signal_tx.clone()
    }

    /// Get reference to book manager
    pub fn book_manager(&self) -> &OrderBookManager {
        &self.book_manager
    }

    /// Get mutable reference to book manager
    pub fn book_manager_mut(&mut self) -> &mut OrderBookManager {
        &mut self.book_manager
    }

    /// Shutdown all strategies
    pub fn shutdown(&mut self) {
        info!(
            "SignalEngine shutting down {} strategies",
            self.strategies.len()
        );
        self.shutdown.store(true, Ordering::SeqCst);

        for strategy in &self.strategies {
            strategy.shutdown();
        }
    }

    /// Wait for all strategy threads to finish
    pub fn join(mut self) {
        for handle in self.strategies.drain(..) {
            if let Err(e) = handle.join() {
                error!("Strategy thread panicked: {:?}", e);
            }
        }
    }

    /// Get number of active strategies
    pub fn strategy_count(&self) -> usize {
        self.strategies.len()
    }

    /// Get names of all strategies
    pub fn strategy_names(&self) -> Vec<&str> {
        self.strategies.iter().map(|s| s.name.as_str()).collect()
    }
}

impl Drop for SignalEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Builder for SignalEngine
#[allow(dead_code)]
pub struct SignalEngineBuilder {
    config: SignalEngineConfig,
    book_manager: Option<OrderBookManager>,
}

#[allow(dead_code)]
impl SignalEngineBuilder {
    pub fn new() -> Self {
        Self {
            config: SignalEngineConfig::default(),
            book_manager: None,
        }
    }

    pub fn config(mut self, config: SignalEngineConfig) -> Self {
        self.config = config;
        self
    }

    pub fn tick_interval_ms(mut self, interval: u64) -> Self {
        self.config.tick_interval_ms = interval;
        self
    }

    pub fn book_manager(mut self, book_manager: OrderBookManager) -> Self {
        self.book_manager = Some(book_manager);
        self
    }

    pub fn build(self) -> SignalEngine {
        let book_manager = self.book_manager.unwrap_or_default();
        SignalEngine::new(self.config, book_manager)
    }
}

impl Default for SignalEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway_in::{OrderBookWriter, QualifiedSymbol};
    use crate::signal::traits::SignalGeneratorConfig;
    use crate::signal::types::{SignalDirection, StrategyId, StrategyType};
    use trading_core::DepthSnapshotEvent;

    /// Simple test strategy that generates signals on every tick
    struct TestStrategy {
        config: SignalGeneratorConfig,
        tick_count: u64,
    }

    impl TestStrategy {
        fn new(name: &str) -> Self {
            Self {
                config: SignalGeneratorConfig::new(
                    StrategyId::new(name),
                    StrategyType::MeanReversion,
                    vec![QualifiedSymbol::new("test", "BTCUSDT")],
                ),
                tick_count: 0,
            }
        }
    }

    impl SignalGenerator for TestStrategy {
        fn config(&self) -> &SignalGeneratorConfig {
            &self.config
        }

        fn on_tick(&mut self, book_manager: &OrderBookManager) -> Vec<Signal> {
            self.tick_count += 1;

            // Only generate signal every 10 ticks
            if !self.tick_count.is_multiple_of(10) {
                return vec![];
            }

            let symbol = &self.config.symbols[0];

            // Try to read the book
            let book = book_manager.book_by_key(symbol);
            if book.is_initialized()
                && let Some(mid) = book.mid_price()
            {
                return vec![
                    Signal::builder(
                        self.config.strategy_id.clone(),
                        self.config.strategy_type,
                        symbol.to_string(),
                    )
                    .buy()
                    .strength(0.5)
                    .confidence(0.8)
                    .prices(mid, mid)
                    .build(),
                ];
            }

            vec![]
        }
    }

    #[tokio::test]
    async fn test_signal_engine_basic() {
        let book_manager = OrderBookManager::new();

        // Set up a book
        let symbol = QualifiedSymbol::new("test", "BTCUSDT");
        book_manager.apply_snapshot(
            &symbol,
            &DepthSnapshotEvent {
                last_update_id: 100,
                bids: vec![["100".to_string(), "10".to_string()]],
                asks: vec![["101".to_string(), "10".to_string()]],
            },
        );

        // Create engine with fast tick rate for testing
        let config = SignalEngineConfig {
            tick_interval_ms: 1,
            ..Default::default()
        };

        let mut engine = SignalEngine::new(config, book_manager);

        // Add test strategy
        engine.add_strategy(TestStrategy::new("test_strat"));

        // Get receiver
        let mut receiver = engine.take_signal_receiver().unwrap();

        // Wait for some signals
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should have received some signals
        let mut signal_count = 0;
        while let Ok(signal) = receiver.try_recv() {
            signal_count += 1;
            assert_eq!(signal.strategy_id.as_str(), "test_strat");
            assert_eq!(signal.direction, SignalDirection::Buy);
        }

        assert!(signal_count > 0, "Should have received at least one signal");

        // Shutdown
        engine.shutdown();
    }

    #[test]
    fn test_signal_engine_builder() {
        let engine = SignalEngineBuilder::new().tick_interval_ms(5).build();

        assert_eq!(engine.config.tick_interval_ms, 5);
        assert_eq!(engine.strategy_count(), 0);
    }
}
