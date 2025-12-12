//! Concrete Strategy Implementations
//!
//! This module contains trading strategy implementations that implement
//! the `SignalGeneratorPort` trait from the application layer.
//!
//! # Available Strategies
//!
//! - `MeanReversionHFT`: High-frequency mean reversion strategy based on
//!   microprice deviation and order book imbalance.

mod mean_reversion_hft;

pub use mean_reversion_hft::{MeanReversionConfig, MeanReversionHFT};
