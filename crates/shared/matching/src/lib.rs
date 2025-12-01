//! Athena Matching Algorithms
//!
//! Implementations of order matching algorithms for the Athena trading system.

mod price_time;
mod pro_rata;

pub use price_time::PriceTimeMatchingEngine;
pub use pro_rata::ProRataMatchingEngine;

// Re-export the trait from ports for convenience
pub use athena_ports::{MatchingAlgorithm, MatchingError, MatchingResult};

/// Factory function to create matching algorithms by name
pub fn create_matching_algorithm(algorithm_type: &str) -> Box<dyn MatchingAlgorithm> {
    match algorithm_type.to_lowercase().as_str() {
        "pro-rata" | "prorata" => Box::new(ProRataMatchingEngine::new()),
        _ => Box::new(PriceTimeMatchingEngine::new()), // Default
    }
}
