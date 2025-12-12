//! Domain Layer - Core business entities and value objects
//!
//! This layer contains pure business logic with no external dependencies.
//! All types here are framework-agnostic and represent core trading concepts.

mod calculations;
mod direction;
mod features;
pub mod order_book;
mod signal;
mod strategy;
mod value_objects;

pub use calculations::{Calculations, Imbalance, Microprice, Spread, Vwap};
pub use direction::SignalDirection;
pub use features::{Features, Leg, Urgency};
pub use order_book::{OrderBookManager, SharedOrderBook};
pub use signal::{Signal, SignalBuilder, SignalId};
pub use strategy::{StrategyId, StrategyType};
pub use value_objects::{BasisPoints, Confidence, Ratio, Strength, Volatility, ZScore};
