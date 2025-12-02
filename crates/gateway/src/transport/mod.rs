//! Transport abstraction layer
//!
//! Provides unified traits for message passing using tokio channels.
//! The trait-based design allows swapping in other transports (NATS, Aeron, etc.) later.

pub mod channel;
pub mod config;

pub use config::Subjects;

use crate::error::TransportError;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

/// Publisher - sends messages to a subject/channel
#[async_trait]
pub trait Publisher<M>: Send + Sync
where
    M: Serialize + Send + Sync,
{
    /// Publish a message
    async fn publish(&self, msg: &M) -> Result<(), TransportError>;

    /// Publish a message to a specific subject (for NATS dynamic subjects)
    async fn publish_to(&self, subject: &str, msg: &M) -> Result<(), TransportError> {
        // Default implementation ignores subject and uses configured subject
        let _ = subject;
        self.publish(msg).await
    }
}

/// Subscriber - receives messages from a subject pattern
#[async_trait]
pub trait Subscriber<M>: Send
where
    M: DeserializeOwned + Send,
{
    /// Wait for the next message
    async fn next(&mut self) -> Result<M, TransportError>;

    /// Try to receive without blocking (returns None if no message available)
    fn try_next(&mut self) -> Result<Option<M>, TransportError>;
}

/// Request/Reply pattern for synchronous-style operations (e.g., order submission)
#[async_trait]
pub trait Requester<Req, Res>: Send + Sync
where
    Req: Serialize + Send + Sync,
    Res: DeserializeOwned + Send,
{
    /// Send a request and wait for a response
    async fn request(&self, req: &Req) -> Result<Res, TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ensure traits are object-safe
    fn _assert_publisher_object_safe(_: &dyn Publisher<String>) {}
    fn _assert_subscriber_object_safe(_: &mut dyn Subscriber<String>) {}
    fn _assert_requester_object_safe(_: &dyn Requester<String, String>) {}
}
