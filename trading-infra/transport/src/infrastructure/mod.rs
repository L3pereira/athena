//! Infrastructure Layer - Concrete implementations
//!
//! Contains concrete transport implementations and configuration.
//! - Channel: In-process communication via crossbeam channels
//! - Aeron: Ultra-low latency reliable messaging (feature-gated)

#[cfg(feature = "channel")]
pub mod channel;
pub mod config;
pub mod factory;

#[cfg(feature = "channel")]
pub use channel::{ChannelPublisher, ChannelSubscriber, channel_pair};
pub use config::{AeronConfig, ChannelConfig, TransportConfig, TransportType};
pub use factory::TransportFactory;
