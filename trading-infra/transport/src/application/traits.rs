//! Transport Traits
//!
//! Core abstractions for message publishing and subscribing.

use super::error::TransportError;

/// Message publisher interface
///
/// Implementations send serialized data to subscribers.
/// All implementations must be thread-safe (Send + Sync).
pub trait Publisher: Send + Sync {
    /// Publish serialized data
    ///
    /// Returns `Ok(())` on success, or an error if the publish failed.
    fn publish(&self, data: &[u8]) -> Result<(), TransportError>;

    /// Publish with topic/channel routing (optional)
    ///
    /// Default implementation ignores the topic and calls `publish()`.
    fn publish_to(&self, topic: &str, data: &[u8]) -> Result<(), TransportError> {
        let _ = topic;
        self.publish(data)
    }

    /// Flush any buffered messages
    ///
    /// Default implementation does nothing (unbuffered).
    fn flush(&self) -> Result<(), TransportError> {
        Ok(())
    }

    /// Check if the publisher is still connected/active
    fn is_active(&self) -> bool {
        true
    }
}

/// Message subscriber interface
///
/// Implementations receive serialized data from publishers.
/// All implementations must be thread-safe (Send + Sync).
pub trait Subscriber: Send + Sync {
    /// Poll for messages, calling handler for each received message
    ///
    /// Returns the count of messages processed.
    /// This is non-blocking - returns immediately if no messages are available.
    fn poll(&self, handler: &mut dyn FnMut(&[u8])) -> Result<usize, TransportError>;

    /// Subscribe to a specific topic (optional)
    ///
    /// Default implementation does nothing (single topic).
    fn subscribe(&self, topic: &str) -> Result<(), TransportError> {
        let _ = topic;
        Ok(())
    }

    /// Non-blocking check if messages are available
    ///
    /// Default returns true (conservative - always poll).
    fn has_messages(&self) -> bool {
        true
    }
}

/// Boxed publisher type for dynamic dispatch
pub type BoxPublisher = Box<dyn Publisher>;

/// Boxed subscriber type for dynamic dispatch
pub type BoxSubscriber = Box<dyn Subscriber>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockPublisher {
        messages: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl Publisher for MockPublisher {
        fn publish(&self, data: &[u8]) -> Result<(), TransportError> {
            self.messages.lock().unwrap().push(data.to_vec());
            Ok(())
        }
    }

    struct MockSubscriber {
        messages: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl Subscriber for MockSubscriber {
        fn poll(&self, handler: &mut dyn FnMut(&[u8])) -> Result<usize, TransportError> {
            let mut msgs = self.messages.lock().unwrap();
            let count = msgs.len();
            for msg in msgs.drain(..) {
                handler(&msg);
            }
            Ok(count)
        }
    }

    #[test]
    fn test_mock_pub_sub() {
        let messages = Arc::new(Mutex::new(Vec::new()));

        let publisher = MockPublisher {
            messages: messages.clone(),
        };
        let subscriber = MockSubscriber { messages };

        // Publish
        publisher.publish(b"hello").unwrap();
        publisher.publish(b"world").unwrap();

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
    }
}
