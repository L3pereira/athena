use crate::domain::ExchangeEvent;
use async_trait::async_trait;

/// Publisher for exchange events (async context)
///
/// Events are published to subscribers (WebSocket connections, message queues, etc.)
/// This decouples the exchange logic from the delivery mechanism.
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publish an event to all subscribers
    async fn publish(&self, event: ExchangeEvent);

    /// Publish an event to subscribers of a specific symbol
    async fn publish_to_symbol(&self, symbol: &str, event: ExchangeEvent);

    /// Get the number of active subscribers
    fn subscriber_count(&self) -> usize;
}

/// Synchronous event sink for use in sync contexts (e.g., shard threads)
///
/// Use this when you need to publish events from non-async code.
/// Implementations should be non-blocking (fire-and-forget).
pub trait SyncEventSink: Send + Sync {
    /// Send an event (non-blocking, best effort)
    fn send(&self, event: ExchangeEvent);
}
