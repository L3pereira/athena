//! Transport Abstraction Layer
//!
//! Provides abstract `Publisher` and `Subscriber` traits for inter-process
//! communication with pluggable implementations:
//!
//! - **Channel** (default): In-process communication via crossbeam channels
//! - **Aeron**: Ultra-low latency reliable messaging via Aeron IPC/UDP (feature-gated)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    Application Layer                     │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
//! │  │  Publisher  │  │  Subscriber │  │   WireMessage   │  │
//! │  │   (trait)   │  │   (trait)   │  │  (wire format)  │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────┘  │
//! └─────────────────────────────────────────────────────────┘
//!                            │
//!                            ▼
//! ┌─────────────────────────────────────────────────────────┐
//! │                  Infrastructure Layer                    │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
//! │  │   Channel   │  │  Aeron IPC  │  │   Aeron UDP     │  │
//! │  │ (crossbeam) │  │  (feature)  │  │   (feature)     │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────┘  │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use transport::{channel_pair, Publisher, Subscriber};
//!
//! // Create a channel pair for in-process communication
//! let (publisher, subscriber) = channel_pair(10000);
//!
//! // Publish data
//! publisher.publish(b"hello").unwrap();
//!
//! // Subscribe to data
//! subscriber.poll(&mut |data| {
//!     println!("Received: {:?}", data);
//! }).unwrap();
//! ```

pub mod application;
pub mod infrastructure;

// Re-export application layer types (ports/abstractions)
pub use application::{
    MSG_DEPTH_UPDATE, MSG_ORDER_BOOK_SNAPSHOT, MSG_SIGNAL, MSG_SNAPSHOT_REQUEST, MSG_TRADE,
    MessageType, Publisher, Subscriber, TransportError, WireMessage,
};

// Re-export infrastructure layer types (implementations)
pub use infrastructure::{ChannelConfig, TransportConfig, TransportFactory, TransportType};

// Re-export channel types when feature is enabled
#[cfg(feature = "channel")]
pub use infrastructure::{ChannelPublisher, ChannelSubscriber, channel_pair};
