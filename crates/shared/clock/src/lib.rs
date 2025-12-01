//! Athena Clock Infrastructure
//!
//! Provides time abstractions for simulation and production:
//!
//! ## Clock Hierarchy
//!
//! ```text
//! WorldClock (universal simulation truth)
//!     │
//!     ├── ExchangeClock (drift: ±X ms from world)
//!     │       │
//!     │       └── AgentTimeView (latency: +Y ms from exchange)
//!     │
//!     └── ExchangeClock (different drift)
//!             │
//!             └── AgentTimeView (different latency)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use athena_clock::{WorldClock, ExchangeClock, AgentTimeView, TimeScale};
//! use chrono::Duration;
//!
//! // Create world clock (simulation root)
//! let world = WorldClock::new(None);
//!
//! // Create exchanges with different drifts
//! let nyse = ExchangeClock::new(world.clone(), Duration::milliseconds(5), "NYSE");
//! let nasdaq = ExchangeClock::new(world.clone(), Duration::milliseconds(-3), "NASDAQ");
//!
//! // Create agents with different latencies
//! let hft_bot = AgentTimeView::new_colocated(nyse.clone(), "HFT-Bot");
//! let retail = AgentTimeView::new_retail(nasdaq.clone(), "Retail-Trader");
//!
//! // Control time for testing
//! world.set_time_scale(TimeScale::Fast(100)).await; // 100x speed
//! world.set_time_scale(TimeScale::Fixed).await;     // Frozen time
//! world.advance(Duration::minutes(5)).await;        // Jump forward
//! ```

mod agent;
mod exchange;
mod system;
mod world;

pub use agent::AgentTimeView;
pub use exchange::ExchangeClock;
pub use system::SystemClock;
pub use world::{TimeScale, WorldClock};

// Re-export the Clock trait for convenience
pub use athena_ports::Clock;
