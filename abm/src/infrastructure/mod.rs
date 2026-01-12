//! Infrastructure Layer
//!
//! Implementations of ports for the ABM simulation.

mod exchange_adapter;
mod mock_feed;

pub use exchange_adapter::ExchangeAdapter;
pub use mock_feed::{MockFeed, MockFeedConfig, Scenario};
