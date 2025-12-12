//! Infrastructure Feature Extractors
//!
//! Concrete implementations of feature extraction using domain calculations
//! and clean architecture ports.

mod order_book_extractor;
mod price_extractor;

pub use order_book_extractor::OrderBookExtractor;
pub use price_extractor::PriceExtractor;
