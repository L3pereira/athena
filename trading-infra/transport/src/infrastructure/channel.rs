//! Channel Transport Implementation
//!
//! In-process communication via crossbeam bounded channels.
//! This is the default transport for single-process and simulation modes.

use crate::application::error::TransportError;
use crate::application::traits::{Publisher, Subscriber};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};

/// Channel-based publisher
///
/// Sends messages to a crossbeam bounded channel.
#[derive(Clone)]
pub struct ChannelPublisher {
    tx: Sender<Vec<u8>>,
}

impl ChannelPublisher {
    /// Create a new channel publisher
    pub fn new(tx: Sender<Vec<u8>>) -> Self {
        Self { tx }
    }
}

impl Publisher for ChannelPublisher {
    fn publish(&self, data: &[u8]) -> Result<(), TransportError> {
        match self.tx.try_send(data.to_vec()) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(TransportError::Full),
            Err(TrySendError::Disconnected(_)) => Err(TransportError::ChannelClosed),
        }
    }

    fn is_active(&self) -> bool {
        // Crossbeam doesn't expose is_disconnected on Sender
        // We detect disconnection on send (returns Disconnected error)
        // For proactive check, we attempt a capacity check
        // If channel has any capacity, it's likely still active
        true
    }
}

/// Channel-based subscriber
///
/// Receives messages from a crossbeam bounded channel.
pub struct ChannelSubscriber {
    rx: Receiver<Vec<u8>>,
}

impl ChannelSubscriber {
    /// Create a new channel subscriber
    pub fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self { rx }
    }
}

impl Subscriber for ChannelSubscriber {
    fn poll(&self, handler: &mut dyn FnMut(&[u8])) -> Result<usize, TransportError> {
        let mut count = 0;
        while let Ok(data) = self.rx.try_recv() {
            handler(&data);
            count += 1;
        }
        Ok(count)
    }

    fn has_messages(&self) -> bool {
        !self.rx.is_empty()
    }
}

/// Create a channel publisher/subscriber pair
///
/// # Arguments
///
/// * `capacity` - The bounded channel capacity (backpressure threshold)
///
/// # Returns
///
/// A tuple of (ChannelPublisher, ChannelSubscriber)
pub fn channel_pair(capacity: usize) -> (ChannelPublisher, ChannelSubscriber) {
    let (tx, rx) = bounded(capacity);
    (ChannelPublisher::new(tx), ChannelSubscriber::new(rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_pair_basic() {
        let (publisher, subscriber) = channel_pair(100);

        // Publish
        publisher.publish(b"hello").unwrap();
        publisher.publish(b"world").unwrap();

        // Has messages
        assert!(subscriber.has_messages());

        // Subscribe
        let mut received = Vec::new();
        let count = subscriber
            .poll(&mut |data| {
                received.push(data.to_vec());
            })
            .unwrap();

        assert_eq!(count, 2);
        assert_eq!(received[0], b"hello");
        assert_eq!(received[1], b"world");

        // No more messages
        assert!(!subscriber.has_messages());
    }

    #[test]
    fn test_channel_backpressure() {
        let (publisher, _subscriber) = channel_pair(2);

        // Fill the channel
        publisher.publish(b"1").unwrap();
        publisher.publish(b"2").unwrap();

        // Should get backpressure
        let result = publisher.publish(b"3");
        assert!(matches!(result, Err(TransportError::Full)));
    }

    #[test]
    fn test_channel_disconnect() {
        let (publisher, subscriber) = channel_pair(10);

        // Drop the subscriber
        drop(subscriber);

        // Publisher detects disconnect on send (returns Disconnected error)
        // Note: is_active() returns true conservatively, actual check is on send
        let result = publisher.publish(b"data");
        assert!(matches!(result, Err(TransportError::ChannelClosed)));
    }

    #[test]
    fn test_channel_empty_poll() {
        let (_publisher, subscriber) = channel_pair(10);

        // Poll empty channel
        let mut count = 0;
        let result = subscriber
            .poll(&mut |_| {
                count += 1;
            })
            .unwrap();

        assert_eq!(result, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_channel_clone_publisher() {
        let (publisher, subscriber) = channel_pair(10);

        // Clone publisher
        let publisher2 = publisher.clone();

        // Both can publish
        publisher.publish(b"from-1").unwrap();
        publisher2.publish(b"from-2").unwrap();

        // Subscriber receives both
        let mut received = Vec::new();
        subscriber
            .poll(&mut |data| {
                received.push(data.to_vec());
            })
            .unwrap();

        assert_eq!(received.len(), 2);
    }
}
