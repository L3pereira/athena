//! Signal Channel Adapter - Adapts tokio channels to SignalPublisher/SignalSubscriber
//!
//! Provides concrete implementations of the signal publishing ports
//! using tokio's unbounded MPSC channels.

use crate::application::ports::{
    PublishError, SignalChannelFactory, SignalPublisher, SignalSubscriber,
};
use crate::domain::Signal;
use tokio::sync::mpsc;

/// SignalPublisher implementation using tokio unbounded channel
pub struct ChannelSignalPublisher {
    sender: mpsc::UnboundedSender<Signal>,
}

impl ChannelSignalPublisher {
    pub fn new(sender: mpsc::UnboundedSender<Signal>) -> Self {
        Self { sender }
    }

    /// Check if the channel is closed
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

impl SignalPublisher for ChannelSignalPublisher {
    fn publish(&self, signal: Signal) -> Result<(), PublishError> {
        self.sender
            .send(signal)
            .map_err(|e| PublishError::new(format!("Channel closed: {}", e)))
    }

    fn is_active(&self) -> bool {
        !self.sender.is_closed()
    }
}

impl Clone for ChannelSignalPublisher {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// SignalSubscriber implementation using tokio unbounded channel
pub struct ChannelSignalSubscriber {
    receiver: mpsc::UnboundedReceiver<Signal>,
}

impl ChannelSignalSubscriber {
    pub fn new(receiver: mpsc::UnboundedReceiver<Signal>) -> Self {
        Self { receiver }
    }

    /// Get the underlying receiver (consumes self)
    pub fn into_inner(self) -> mpsc::UnboundedReceiver<Signal> {
        self.receiver
    }
}

impl SignalSubscriber for ChannelSignalSubscriber {
    async fn recv(&mut self) -> Option<Signal> {
        self.receiver.recv().await
    }

    fn try_recv(&mut self) -> Option<Signal> {
        self.receiver.try_recv().ok()
    }
}

/// Factory for creating channel-based publisher/subscriber pairs
pub struct ChannelFactory;

impl ChannelFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ChannelFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalChannelFactory for ChannelFactory {
    type Publisher = ChannelSignalPublisher;
    type Subscriber = ChannelSignalSubscriber;

    fn create(&self) -> (Self::Publisher, Self::Subscriber) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            ChannelSignalPublisher::new(tx),
            ChannelSignalSubscriber::new(rx),
        )
    }
}

/// Convenience function to create a signal channel pair
pub fn create_signal_channel() -> (ChannelSignalPublisher, ChannelSignalSubscriber) {
    ChannelFactory::new().create()
}

/// A bounded channel variant for backpressure control
pub struct BoundedChannelFactory {
    buffer_size: usize,
}

impl BoundedChannelFactory {
    pub fn new(buffer_size: usize) -> Self {
        Self { buffer_size }
    }
}

/// SignalPublisher implementation using bounded channel
pub struct BoundedSignalPublisher {
    sender: mpsc::Sender<Signal>,
}

impl BoundedSignalPublisher {
    pub fn new(sender: mpsc::Sender<Signal>) -> Self {
        Self { sender }
    }
}

impl SignalPublisher for BoundedSignalPublisher {
    fn publish(&self, signal: Signal) -> Result<(), PublishError> {
        // Use try_send to avoid blocking in sync context
        self.sender
            .try_send(signal)
            .map_err(|e| PublishError::new(format!("Channel error: {}", e)))
    }

    fn is_active(&self) -> bool {
        !self.sender.is_closed()
    }
}

impl Clone for BoundedSignalPublisher {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// SignalSubscriber implementation using bounded channel
pub struct BoundedSignalSubscriber {
    receiver: mpsc::Receiver<Signal>,
}

impl BoundedSignalSubscriber {
    pub fn new(receiver: mpsc::Receiver<Signal>) -> Self {
        Self { receiver }
    }
}

impl SignalSubscriber for BoundedSignalSubscriber {
    async fn recv(&mut self) -> Option<Signal> {
        self.receiver.recv().await
    }

    fn try_recv(&mut self) -> Option<Signal> {
        self.receiver.try_recv().ok()
    }
}

impl SignalChannelFactory for BoundedChannelFactory {
    type Publisher = BoundedSignalPublisher;
    type Subscriber = BoundedSignalSubscriber;

    fn create(&self) -> (Self::Publisher, Self::Subscriber) {
        let (tx, rx) = mpsc::channel(self.buffer_size);
        (
            BoundedSignalPublisher::new(tx),
            BoundedSignalSubscriber::new(rx),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{SignalDirection, StrategyId, StrategyType};
    use trading_core::Price;

    fn create_test_signal() -> Signal {
        Signal::builder(
            StrategyId::new("test"),
            StrategyType::MeanReversion,
            "BTCUSDT".to_string(),
        )
        .direction(SignalDirection::Buy)
        .strength(0.5)
        .confidence(0.8)
        .prices(Price::from_int(100), Price::from_int(100))
        .build()
    }

    #[tokio::test]
    async fn test_channel_publisher_subscriber() {
        let (publisher, mut subscriber) = create_signal_channel();

        let signal = create_test_signal();
        publisher.publish(signal.clone()).unwrap();

        let received = subscriber.recv().await.unwrap();
        assert_eq!(received.strategy_id.as_str(), "test");
        assert_eq!(received.direction, SignalDirection::Buy);
    }

    #[tokio::test]
    async fn test_try_recv() {
        let (publisher, mut subscriber) = create_signal_channel();

        // Empty - should return None
        assert!(subscriber.try_recv().is_none());

        // Publish and try_recv
        publisher.publish(create_test_signal()).unwrap();
        let received = subscriber.try_recv();
        assert!(received.is_some());
    }

    #[test]
    fn test_publisher_is_active() {
        let (publisher, subscriber) = create_signal_channel();

        assert!(publisher.is_active());

        // Drop subscriber - publisher should detect closed
        drop(subscriber);
        assert!(!publisher.is_active());
    }

    #[tokio::test]
    async fn test_bounded_channel() {
        let factory = BoundedChannelFactory::new(10);
        let (publisher, mut subscriber) = factory.create();

        publisher.publish(create_test_signal()).unwrap();

        let received = subscriber.recv().await.unwrap();
        assert_eq!(received.strategy_id.as_str(), "test");
    }

    #[test]
    fn test_bounded_channel_backpressure() {
        let factory = BoundedChannelFactory::new(2);
        let (publisher, _subscriber) = factory.create();

        // Fill the buffer
        publisher.publish(create_test_signal()).unwrap();
        publisher.publish(create_test_signal()).unwrap();

        // Third should fail due to backpressure
        let result = publisher.publish(create_test_signal());
        assert!(result.is_err());
    }
}
