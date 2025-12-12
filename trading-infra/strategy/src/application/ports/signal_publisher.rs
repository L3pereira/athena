//! Signal Publisher Port - Abstraction for signal output
//!
//! This port defines how signals are published/sent downstream.
//! Allows different output mechanisms (channels, queues, etc.)

use crate::domain::Signal;

/// Error type for signal publishing failures
#[derive(Debug, Clone)]
pub struct PublishError {
    pub message: String,
}

impl PublishError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PublishError: {}", self.message)
    }
}

impl std::error::Error for PublishError {}

/// Port for publishing signals
///
/// Implementations can send signals to:
/// - MPSC channels (for async processing)
/// - Message queues (for distributed systems)
/// - Direct callbacks (for testing)
/// - Aggregators (for combining signals)
pub trait SignalPublisher: Send + Sync {
    /// Publish a single signal
    fn publish(&self, signal: Signal) -> Result<(), PublishError>;

    /// Publish multiple signals
    fn publish_batch(&self, signals: Vec<Signal>) -> Result<(), PublishError> {
        for signal in signals {
            self.publish(signal)?;
        }
        Ok(())
    }

    /// Check if publisher is still active/connected
    fn is_active(&self) -> bool {
        true
    }
}

/// Port for receiving signals (input side of the channel)
///
/// Used by downstream consumers (execution, aggregation, etc.)
#[allow(async_fn_in_trait)]
pub trait SignalSubscriber: Send {
    /// Receive the next signal (blocking)
    async fn recv(&mut self) -> Option<Signal>;

    /// Try to receive a signal (non-blocking)
    fn try_recv(&mut self) -> Option<Signal>;
}

/// Factory for creating publisher/subscriber pairs
pub trait SignalChannelFactory {
    type Publisher: SignalPublisher;
    type Subscriber: SignalSubscriber;

    /// Create a new channel pair
    fn create(&self) -> (Self::Publisher, Self::Subscriber);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Mock publisher for testing
    #[allow(dead_code)]
    struct MockPublisher {
        signals: Arc<Mutex<Vec<Signal>>>,
    }

    impl SignalPublisher for MockPublisher {
        fn publish(&self, signal: Signal) -> Result<(), PublishError> {
            self.signals.lock().unwrap().push(signal);
            Ok(())
        }
    }

    #[test]
    fn test_publish_error() {
        let err = PublishError::new("channel closed");
        assert_eq!(err.to_string(), "PublishError: channel closed");
    }
}
