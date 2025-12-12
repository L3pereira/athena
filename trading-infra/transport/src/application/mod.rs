//! Application Layer - Ports and abstractions
//!
//! Contains the abstract traits and types that define the transport interface.
//! Other crates depend on these abstractions, not concrete implementations.

pub mod error;
pub mod message;
pub mod traits;

pub use error::TransportError;
pub use message::{
    MSG_DEPTH_UPDATE, MSG_ORDER_BOOK_SNAPSHOT, MSG_SIGNAL, MSG_SNAPSHOT_REQUEST, MSG_TRADE,
    MessageType, WireMessage,
};
pub use traits::{Publisher, Subscriber};
