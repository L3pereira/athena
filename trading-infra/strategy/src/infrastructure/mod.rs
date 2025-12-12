//! Infrastructure Layer - Concrete implementations
//!
//! The infrastructure layer provides concrete implementations of:
//! - Adapters: Connect domain/application to external systems
//! - Extractors: Feature extraction implementations
//! - Strategies: Trading strategy implementations
//! - Subscriber: Market data transport subscriber
//!
//! This layer depends on application (ports) and domain layers.
//! External frameworks and libraries are used here.

pub mod adapters;
pub mod extractors;
pub mod strategies;
pub mod subscriber;

pub use adapters::*;
pub use extractors::*;
pub use strategies::*;
pub use subscriber::MarketDataSubscriber;
