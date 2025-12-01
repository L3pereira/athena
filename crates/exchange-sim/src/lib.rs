// Application layer
pub mod application;

// Infrastructure layer
pub mod infrastructure;

// Cross-cutting concerns
pub mod error;
pub mod model;

// Re-export main types for convenience
pub use application::Exchange;
pub use infrastructure::{OrderBook, Router, time};

// Tests
mod tests;
