//! Signal generation traits
//!
//! Defines the interfaces for signal generators and feature extractors.

use crate::gateway_in::QualifiedSymbol;
use crate::order_book::{OrderBookManager, SharedOrderBook};
use crate::signal::types::{Signal, StrategyId, StrategyType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Configuration for a signal generator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalGeneratorConfig {
    /// Strategy identifier
    pub strategy_id: StrategyId,
    /// Strategy type classification
    pub strategy_type: StrategyType,
    /// Symbols this strategy watches
    pub symbols: Vec<QualifiedSymbol>,
    /// Minimum interval between signals (milliseconds)
    pub min_signal_interval_ms: u64,
    /// Strategy-specific parameters
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

impl SignalGeneratorConfig {
    pub fn new(
        strategy_id: StrategyId,
        strategy_type: StrategyType,
        symbols: Vec<QualifiedSymbol>,
    ) -> Self {
        Self {
            strategy_id,
            strategy_type,
            symbols,
            min_signal_interval_ms: 100,
            params: HashMap::new(),
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.params.insert(key.into(), value);
        self
    }

    pub fn with_min_interval(mut self, interval_ms: u64) -> Self {
        self.min_signal_interval_ms = interval_ms;
        self
    }

    pub fn get_param<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.params
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Trait for signal generators (strategies)
///
/// Implementations run on dedicated OS threads and generate trading signals
/// based on market data. The `on_book_update` method is called whenever
/// any watched order book changes.
pub trait SignalGenerator: Send + 'static {
    /// Get the strategy configuration
    fn config(&self) -> &SignalGeneratorConfig;

    /// Called on each tick/update cycle
    ///
    /// Implementations should:
    /// 1. Read order books (lock-free via ArcSwap)
    /// 2. Extract features
    /// 3. Generate signals based on strategy logic
    /// 4. Return signals (empty vec if no signal)
    fn on_tick(&mut self, book_manager: &OrderBookManager) -> Vec<Signal>;

    /// Called when a specific order book is updated
    ///
    /// This allows strategies to react immediately to book changes
    /// rather than waiting for the next tick.
    fn on_book_update(
        &mut self,
        symbol: &QualifiedSymbol,
        book: &SharedOrderBook,
    ) -> Option<Signal> {
        // Default: no immediate reaction, wait for tick
        let _ = (symbol, book);
        None
    }

    /// Get strategy name for logging
    fn name(&self) -> &str {
        self.config().strategy_id.as_str()
    }

    /// Get watched symbols
    fn symbols(&self) -> &[QualifiedSymbol] {
        &self.config().symbols
    }
}

/// Trait for extracting features from order book data
///
/// Feature extractors transform raw order book data into numerical features
/// that can be used by signal generators for decision making.
pub trait FeatureExtractor: Send + Sync {
    /// Extract features from an order book
    fn extract(&self, book: &SharedOrderBook) -> HashMap<String, f64>;

    /// Get feature names this extractor produces
    fn feature_names(&self) -> &[&str];
}

/// Combined features from multiple extractors
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct CombinedFeatures {
    features: HashMap<String, f64>,
}

#[allow(dead_code)]
impl CombinedFeatures {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn merge(&mut self, other: HashMap<String, f64>) {
        self.features.extend(other);
    }

    pub fn get(&self, name: &str) -> Option<f64> {
        self.features.get(name).copied()
    }

    pub fn into_inner(self) -> HashMap<String, f64> {
        self.features
    }

    pub fn as_map(&self) -> &HashMap<String, f64> {
        &self.features
    }
}

/// Feature extraction pipeline
///
/// Combines multiple feature extractors into a single pipeline.
#[allow(dead_code)]
pub struct FeaturePipeline {
    extractors: Vec<Box<dyn FeatureExtractor>>,
}

#[allow(dead_code)]
impl FeaturePipeline {
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }

    pub fn add<E: FeatureExtractor + 'static>(mut self, extractor: E) -> Self {
        self.extractors.push(Box::new(extractor));
        self
    }

    pub fn extract(&self, book: &SharedOrderBook) -> CombinedFeatures {
        let mut combined = CombinedFeatures::new();
        for extractor in &self.extractors {
            combined.merge(extractor.extract(book));
        }
        combined
    }

    pub fn extract_multi(
        &self,
        books: &[(QualifiedSymbol, Arc<SharedOrderBook>)],
    ) -> HashMap<QualifiedSymbol, CombinedFeatures> {
        books
            .iter()
            .map(|(sym, book)| (sym.clone(), self.extract(book)))
            .collect()
    }
}

impl Default for FeaturePipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle for sending signals from a strategy thread
#[allow(dead_code)]
#[derive(Clone)]
pub struct SignalSender {
    sender: mpsc::UnboundedSender<Signal>,
    strategy_id: StrategyId,
}

#[allow(dead_code)]
impl SignalSender {
    pub fn new(sender: mpsc::UnboundedSender<Signal>, strategy_id: StrategyId) -> Self {
        Self {
            sender,
            strategy_id,
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn send(&self, signal: Signal) -> Result<(), mpsc::error::SendError<Signal>> {
        self.sender.send(signal)
    }

    pub fn strategy_id(&self) -> &StrategyId {
        &self.strategy_id
    }
}

/// Handle for receiving signals
pub struct SignalReceiver {
    receiver: mpsc::UnboundedReceiver<Signal>,
}

impl SignalReceiver {
    pub fn new(receiver: mpsc::UnboundedReceiver<Signal>) -> Self {
        Self { receiver }
    }

    pub async fn recv(&mut self) -> Option<Signal> {
        self.receiver.recv().await
    }

    pub fn try_recv(&mut self) -> Result<Signal, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }
}

/// Create a signal channel pair
#[allow(dead_code)]
pub fn signal_channel(strategy_id: StrategyId) -> (SignalSender, SignalReceiver) {
    let (tx, rx) = mpsc::unbounded_channel();
    (SignalSender::new(tx, strategy_id), SignalReceiver::new(rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExtractor;

    impl FeatureExtractor for MockExtractor {
        fn extract(&self, _book: &SharedOrderBook) -> HashMap<String, f64> {
            let mut features = HashMap::new();
            features.insert("mock_feature".to_string(), 1.0);
            features
        }

        fn feature_names(&self) -> &[&str] {
            &["mock_feature"]
        }
    }

    #[test]
    fn test_feature_pipeline() {
        let pipeline = FeaturePipeline::new().add(MockExtractor);
        let manager = OrderBookManager::new();
        let book = manager.book("test", "BTCUSDT");

        let features = pipeline.extract(&book);
        assert_eq!(features.get("mock_feature"), Some(1.0));
    }

    #[test]
    fn test_combined_features() {
        let mut combined = CombinedFeatures::new();

        let mut features1 = HashMap::new();
        features1.insert("a".to_string(), 1.0);

        let mut features2 = HashMap::new();
        features2.insert("b".to_string(), 2.0);

        combined.merge(features1);
        combined.merge(features2);

        assert_eq!(combined.get("a"), Some(1.0));
        assert_eq!(combined.get("b"), Some(2.0));
    }
}
