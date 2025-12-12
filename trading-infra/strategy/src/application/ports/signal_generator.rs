//! Signal Generator Port - Abstraction for trading strategies
//!
//! Defines the interface for signal generation strategies.
//! Strategies implement this port and run on dedicated threads.

use crate::application::ports::market_data::{MarketDataPort, SymbolKey};
use crate::domain::{Signal, StrategyId, StrategyType};

/// Configuration for a signal generator
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Strategy identifier
    pub strategy_id: StrategyId,
    /// Strategy type classification
    pub strategy_type: StrategyType,
    /// Symbols this strategy monitors
    pub symbols: Vec<SymbolKey>,
    /// Minimum interval between signals (milliseconds)
    pub min_signal_interval_ms: u64,
}

impl GeneratorConfig {
    pub fn new(
        strategy_id: StrategyId,
        strategy_type: StrategyType,
        symbols: Vec<SymbolKey>,
    ) -> Self {
        Self {
            strategy_id,
            strategy_type,
            symbols,
            min_signal_interval_ms: 100,
        }
    }

    pub fn with_min_interval(mut self, interval_ms: u64) -> Self {
        self.min_signal_interval_ms = interval_ms;
        self
    }
}

/// Port for signal generation strategies
///
/// This is the core abstraction for trading strategies. Implementations:
/// - Receive market data through the MarketDataPort
/// - Generate trading signals based on strategy logic
/// - Can maintain internal state
///
/// Strategies run on dedicated OS threads for isolation.
pub trait SignalGeneratorPort: Send + 'static {
    /// Get strategy configuration
    fn config(&self) -> &GeneratorConfig;

    /// Called on each tick/update cycle
    ///
    /// The strategy should:
    /// 1. Read relevant order books from market_data
    /// 2. Extract features / analyze data
    /// 3. Generate signals based on strategy logic
    /// 4. Return signals (empty vec if no signal)
    fn on_tick<M: MarketDataPort>(&mut self, market_data: &M) -> Vec<Signal>;

    /// Get strategy name (convenience method)
    fn name(&self) -> &str {
        self.config().strategy_id.as_str()
    }

    /// Get watched symbols
    fn symbols(&self) -> &[SymbolKey] {
        &self.config().symbols
    }

    /// Called when strategy is starting
    fn on_start(&mut self) {}

    /// Called when strategy is stopping
    fn on_stop(&mut self) {}
}

/// Factory trait for creating signal generators
///
/// Generic factory that produces a specific generator type.
/// Uses associated types for type safety without dyn dispatch.
pub trait SignalGeneratorFactory<G: SignalGeneratorPort>: Send + Sync {
    /// Create a signal generator from configuration
    fn create(&self, config: GeneratorConfig) -> G;

    /// Get the strategy type this factory creates
    fn strategy_type(&self) -> StrategyType;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generator_config() {
        let config = GeneratorConfig::new(
            StrategyId::new("test"),
            StrategyType::MeanReversion,
            vec![SymbolKey::new("binance", "BTCUSDT")],
        )
        .with_min_interval(50);

        assert_eq!(config.strategy_id.as_str(), "test");
        assert_eq!(config.min_signal_interval_ms, 50);
        assert_eq!(config.symbols.len(), 1);
    }
}
