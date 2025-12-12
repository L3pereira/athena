//! Application Ports - Interfaces for the signal generation domain
//!
//! Ports define the boundaries of the application layer:
//! - Input Ports: How external actors interact with the domain (SignalGenerator)
//! - Output Ports: How the domain interacts with external systems (MarketData, SignalPublisher)
//!
//! Following hexagonal/clean architecture, the domain depends on these abstractions,
//! and infrastructure provides concrete implementations.

mod feature_extractor;
mod market_data;
mod signal_generator;
mod signal_publisher;

pub use feature_extractor::{
    FeatureExtractionPipeline, FeatureExtractorPort, StatefulFeatureExtractor,
};
pub use market_data::{BookLevel, MarketDataPort, OrderBookReader, SymbolKey};
pub use signal_generator::{GeneratorConfig, SignalGeneratorFactory, SignalGeneratorPort};
pub use signal_publisher::{PublishError, SignalChannelFactory, SignalPublisher, SignalSubscriber};
