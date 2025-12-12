//! Feature Extractor Port - Abstraction for feature extraction
//!
//! Defines how features are extracted from market data.
//! Follows Interface Segregation - extractors only need OrderBookReader.

use crate::signal::application::ports::market_data::OrderBookReader;
use crate::signal::domain::Features;
use std::sync::Arc;

/// Port for extracting features from order book data
///
/// Feature extractors transform raw market data into numerical features
/// used by signal generators. They should be:
/// - Stateless (or clearly documented if stateful)
/// - Pure functions of their input
/// - Thread-safe (Send + Sync)
pub trait FeatureExtractorPort: Send + Sync {
    /// Extract features from an order book
    ///
    /// Returns a Features container with named feature values.
    fn extract(&self, book: &dyn OrderBookReader) -> Features;

    /// Get the names of features this extractor produces
    fn feature_names(&self) -> Vec<&'static str>;

    /// Get extractor name for identification
    fn name(&self) -> &'static str;
}

/// Pipeline for combining multiple feature extractors
pub struct FeatureExtractionPipeline {
    extractors: Vec<Arc<dyn FeatureExtractorPort>>,
}

impl FeatureExtractionPipeline {
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }

    /// Add an extractor to the pipeline
    pub fn with_extractor(mut self, extractor: Arc<dyn FeatureExtractorPort>) -> Self {
        self.extractors.push(extractor);
        self
    }

    /// Extract all features from an order book
    pub fn extract(&self, book: &dyn OrderBookReader) -> Features {
        let mut combined = Features::new();
        for extractor in &self.extractors {
            combined.merge(extractor.extract(book));
        }
        combined
    }

    /// Get all feature names from all extractors
    pub fn all_feature_names(&self) -> Vec<&'static str> {
        self.extractors
            .iter()
            .flat_map(|e| e.feature_names())
            .collect()
    }

    /// Get number of extractors in pipeline
    pub fn extractor_count(&self) -> usize {
        self.extractors.len()
    }
}

impl Default for FeatureExtractionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for stateful feature extractors that maintain history
///
/// Some features (EMA, rolling volatility) require state across updates.
/// This trait extends FeatureExtractorPort with state management.
pub trait StatefulFeatureExtractor: Send {
    /// Update state with new data and return features
    fn update(&mut self, mid_price: f64) -> Features;

    /// Reset internal state
    fn reset(&mut self);

    /// Get the names of features this extractor produces
    fn feature_names(&self) -> Vec<&'static str>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::application::ports::market_data::BookLevel;
    use crate::signal::domain::Features;
    use trading_core::{Price, Quantity};

    struct MockBookReader {
        mid: Price,
    }

    impl OrderBookReader for MockBookReader {
        fn is_initialized(&self) -> bool {
            true
        }
        fn best_bid(&self) -> Option<BookLevel> {
            None
        }
        fn best_ask(&self) -> Option<BookLevel> {
            None
        }
        fn mid_price(&self) -> Option<Price> {
            Some(self.mid)
        }
        fn spread(&self) -> Option<Price> {
            None
        }
        fn bid_levels(&self, _depth: usize) -> Vec<BookLevel> {
            vec![]
        }
        fn ask_levels(&self, _depth: usize) -> Vec<BookLevel> {
            vec![]
        }
        fn total_bid_depth(&self, _levels: usize) -> Quantity {
            Quantity::ZERO
        }
        fn total_ask_depth(&self, _levels: usize) -> Quantity {
            Quantity::ZERO
        }
        fn last_update_time(&self) -> Option<u64> {
            None
        }
    }

    struct MockExtractor;

    impl FeatureExtractorPort for MockExtractor {
        fn extract(&self, book: &dyn OrderBookReader) -> Features {
            let mut features = Features::new();
            if let Some(mid) = book.mid_price() {
                features.set("mid_price", mid.to_f64());
            }
            features
        }

        fn feature_names(&self) -> Vec<&'static str> {
            vec!["mid_price"]
        }

        fn name(&self) -> &'static str {
            "mock"
        }
    }

    #[test]
    fn test_feature_pipeline() {
        let pipeline = FeatureExtractionPipeline::new().with_extractor(Arc::new(MockExtractor));

        let book = MockBookReader {
            mid: Price::from_int(100),
        };
        let features = pipeline.extract(&book);

        assert_eq!(features.get("mid_price"), Some(100.0));
        assert_eq!(pipeline.extractor_count(), 1);
    }
}
