//! Transport Factory
//!
//! Creates Publisher/Subscriber pairs from configuration.

use super::config::{TransportConfig, TransportType};
use crate::application::error::TransportError;
use crate::application::traits::{BoxPublisher, BoxSubscriber, Publisher, Subscriber};

/// Factory for creating transport instances from configuration
pub struct TransportFactory;

impl TransportFactory {
    /// Create a publisher from configuration
    #[cfg(feature = "channel")]
    pub fn create_publisher(config: &TransportConfig) -> Result<BoxPublisher, TransportError> {
        match config.transport_type {
            TransportType::Channel => {
                // For publisher-only, we create a channel pair and return the publisher
                // The subscriber is dropped (messages will error when channel is full)
                let (pub_, _sub) = super::channel::channel_pair(config.channel.capacity);
                Ok(Box::new(pub_))
            }
            TransportType::AeronIpc | TransportType::AeronUdp => {
                #[cfg(feature = "aeron")]
                {
                    todo!("Aeron publisher not yet implemented")
                }
                #[cfg(not(feature = "aeron"))]
                {
                    Err(TransportError::Config(
                        "Aeron transport not enabled. Enable 'aeron' feature.".to_string(),
                    ))
                }
            }
        }
    }

    /// Create a subscriber from configuration
    #[cfg(feature = "channel")]
    pub fn create_subscriber(config: &TransportConfig) -> Result<BoxSubscriber, TransportError> {
        match config.transport_type {
            TransportType::Channel => {
                // For subscriber-only, we create a channel pair and return the subscriber
                // The publisher is dropped (no messages will be received)
                let (_pub, sub) = super::channel::channel_pair(config.channel.capacity);
                Ok(Box::new(sub))
            }
            TransportType::AeronIpc | TransportType::AeronUdp => {
                #[cfg(feature = "aeron")]
                {
                    todo!("Aeron subscriber not yet implemented")
                }
                #[cfg(not(feature = "aeron"))]
                {
                    Err(TransportError::Config(
                        "Aeron transport not enabled. Enable 'aeron' feature.".to_string(),
                    ))
                }
            }
        }
    }

    /// Create a matched publisher/subscriber pair
    ///
    /// This is the preferred method for creating connected transports.
    #[cfg(feature = "channel")]
    pub fn create_pair(
        config: &TransportConfig,
    ) -> Result<(BoxPublisher, BoxSubscriber), TransportError> {
        match config.transport_type {
            TransportType::Channel => {
                let (pub_, sub) = super::channel::channel_pair(config.channel.capacity);
                Ok((Box::new(pub_), Box::new(sub)))
            }
            TransportType::AeronIpc | TransportType::AeronUdp => {
                #[cfg(feature = "aeron")]
                {
                    todo!("Aeron pair not yet implemented")
                }
                #[cfg(not(feature = "aeron"))]
                {
                    Err(TransportError::Config(
                        "Aeron transport not enabled. Enable 'aeron' feature.".to_string(),
                    ))
                }
            }
        }
    }

    /// Create a channel pair directly (convenience method)
    #[cfg(feature = "channel")]
    pub fn channel_pair(capacity: usize) -> (impl Publisher + Clone, impl Subscriber) {
        super::channel::channel_pair(capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_channel_pair() {
        let config = TransportConfig::channel(1000);
        let (publisher, subscriber) = TransportFactory::create_pair(&config).unwrap();

        // Test publish
        publisher.publish(b"test").unwrap();

        // Test subscribe
        let mut received = Vec::new();
        subscriber
            .poll(&mut |data| {
                received.push(data.to_vec());
            })
            .unwrap();

        assert_eq!(received.len(), 1);
        assert_eq!(received[0], b"test");
    }

    #[test]
    fn test_create_channel_pair_direct() {
        let (publisher, subscriber) = TransportFactory::channel_pair(1000);

        publisher.publish(b"hello").unwrap();

        let mut count = 0;
        subscriber
            .poll(&mut |_| {
                count += 1;
            })
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    #[cfg(not(feature = "aeron"))]
    fn test_aeron_ipc_disabled() {
        let config = TransportConfig::aeron_ipc(1001);
        let result = TransportFactory::create_pair(&config);
        assert!(result.is_err());
    }

    #[test]
    #[cfg(not(feature = "aeron"))]
    fn test_aeron_udp_disabled() {
        let config = TransportConfig::aeron_udp("localhost:40123", 1002);
        let result = TransportFactory::create_pair(&config);
        assert!(result.is_err());
    }
}
