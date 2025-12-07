use crate::application::ports::{EventPublisher, SyncEventSink};
use crate::domain::ExchangeEvent;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::broadcast;

/// Broadcast-based event publisher
///
/// Uses tokio broadcast channels to publish events to multiple subscribers.
/// Supports both global subscriptions and per-symbol subscriptions.
pub struct BroadcastEventPublisher {
    /// Global broadcast channel for all events
    global_tx: broadcast::Sender<ExchangeEvent>,
    /// Per-symbol broadcast channels
    symbol_channels: Arc<DashMap<String, broadcast::Sender<ExchangeEvent>>>,
    /// Subscriber count
    subscriber_count: Arc<AtomicUsize>,
    /// Channel capacity
    capacity: usize,
}

impl BroadcastEventPublisher {
    pub fn new(capacity: usize) -> Self {
        let (global_tx, _) = broadcast::channel(capacity);

        BroadcastEventPublisher {
            global_tx,
            symbol_channels: Arc::new(DashMap::new()),
            subscriber_count: Arc::new(AtomicUsize::new(0)),
            capacity,
        }
    }

    /// Subscribe to all events
    pub fn subscribe(&self) -> broadcast::Receiver<ExchangeEvent> {
        self.subscriber_count.fetch_add(1, Ordering::SeqCst);
        self.global_tx.subscribe()
    }

    /// Subscribe to events for a specific symbol
    pub fn subscribe_symbol(&self, symbol: &str) -> broadcast::Receiver<ExchangeEvent> {
        self.subscriber_count.fetch_add(1, Ordering::SeqCst);

        let entry = self
            .symbol_channels
            .entry(symbol.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(self.capacity);
                tx
            });

        entry.value().subscribe()
    }

    /// Unsubscribe (decrement counter)
    pub fn unsubscribe(&self) {
        self.subscriber_count.fetch_sub(1, Ordering::SeqCst);
    }
}

/// SyncEventSink implementation for use in sync contexts (e.g., shard threads)
impl SyncEventSink for BroadcastEventPublisher {
    fn send(&self, event: ExchangeEvent) {
        // Non-blocking send, ignore errors (no subscribers)
        let _ = self.global_tx.send(event);
    }
}

impl Default for BroadcastEventPublisher {
    fn default() -> Self {
        Self::new(10000)
    }
}

impl Clone for BroadcastEventPublisher {
    fn clone(&self) -> Self {
        BroadcastEventPublisher {
            global_tx: self.global_tx.clone(),
            symbol_channels: Arc::clone(&self.symbol_channels),
            subscriber_count: Arc::clone(&self.subscriber_count),
            capacity: self.capacity,
        }
    }
}

#[async_trait]
impl EventPublisher for BroadcastEventPublisher {
    async fn publish(&self, event: ExchangeEvent) {
        // Ignore send errors (no subscribers)
        let _ = self.global_tx.send(event);
    }

    async fn publish_to_symbol(&self, symbol: &str, event: ExchangeEvent) {
        // Publish to global channel
        let _ = self.global_tx.send(event.clone());

        // Publish to symbol-specific channel if exists
        if let Some(tx) = self.symbol_channels.get(symbol) {
            let _ = tx.send(event);
        }
    }

    fn subscriber_count(&self) -> usize {
        self.subscriber_count.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderAcceptedEvent, Price, Quantity, Side, Symbol};

    fn create_test_event() -> ExchangeEvent {
        ExchangeEvent::OrderAccepted(OrderAcceptedEvent {
            order_id: uuid::Uuid::new_v4(),
            client_order_id: None,
            symbol: Symbol::new("BTCUSDT").unwrap(),
            side: Side::Buy,
            price: Some(Price::from(rust_decimal::Decimal::from(100))),
            quantity: Quantity::from(rust_decimal::Decimal::from(1)),
            timestamp: chrono::Utc::now(),
        })
    }

    #[tokio::test]
    async fn test_subscribe_and_receive() {
        let publisher = BroadcastEventPublisher::new(100);
        let mut rx = publisher.subscribe();

        let event = create_test_event();
        publisher.publish(event.clone()).await;

        let received = rx.recv().await.unwrap();
        match (received, event) {
            (ExchangeEvent::OrderAccepted(r), ExchangeEvent::OrderAccepted(e)) => {
                assert_eq!(r.order_id, e.order_id);
            }
            _ => panic!("Event mismatch"),
        }
    }

    #[tokio::test]
    async fn test_symbol_subscription() {
        let publisher = BroadcastEventPublisher::new(100);
        let mut btc_rx = publisher.subscribe_symbol("BTCUSDT");
        let mut eth_rx = publisher.subscribe_symbol("ETHUSDT");

        let event = create_test_event();
        publisher.publish_to_symbol("BTCUSDT", event).await;

        // BTC subscriber should receive
        assert!(btc_rx.try_recv().is_ok());

        // ETH subscriber should not receive
        assert!(eth_rx.try_recv().is_err());
    }
}
