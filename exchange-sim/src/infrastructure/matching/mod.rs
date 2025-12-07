// Matching engine placeholder
// The OrderBook entity already handles price-time priority matching
// This module is for future extensibility (e.g., pro-rata matching, auction matching)

pub mod price_time;

pub use price_time::PriceTimeMatcher;
