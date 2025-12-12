//! Wire Message Format
//!
//! Defines the envelope format for messages sent over transport.

use serde::{Deserialize, Serialize};

/// Message type constants
pub const MSG_ORDER_BOOK_SNAPSHOT: u8 = 1;
pub const MSG_DEPTH_UPDATE: u8 = 2;
pub const MSG_TRADE: u8 = 3;
pub const MSG_SIGNAL: u8 = 4;
pub const MSG_SNAPSHOT_REQUEST: u8 = 5;

/// Message type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    OrderBookSnapshot = 1,
    DepthUpdate = 2,
    Trade = 3,
    Signal = 4,
    SnapshotRequest = 5,
}

impl From<u8> for MessageType {
    fn from(value: u8) -> Self {
        match value {
            1 => MessageType::OrderBookSnapshot,
            2 => MessageType::DepthUpdate,
            3 => MessageType::Trade,
            4 => MessageType::Signal,
            5 => MessageType::SnapshotRequest,
            _ => MessageType::DepthUpdate, // Default fallback
        }
    }
}

impl From<MessageType> for u8 {
    fn from(value: MessageType) -> Self {
        value as u8
    }
}

/// Wire message envelope
///
/// All messages sent over transport are wrapped in this envelope.
/// The payload is serialized separately (usually with bincode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessage {
    /// Message type discriminator
    pub msg_type: u8,
    /// Sequence number for ordering/gap detection
    pub sequence: u64,
    /// Timestamp in nanoseconds since epoch
    pub timestamp_ns: u64,
    /// Source identifier (e.g., "gateway-1", "binance")
    pub source: String,
    /// Serialized payload
    pub payload: Vec<u8>,
}

impl WireMessage {
    /// Create a new wire message
    pub fn new<T: Serialize>(
        msg_type: MessageType,
        sequence: u64,
        source: &str,
        payload: &T,
    ) -> Result<Self, bincode::Error> {
        Ok(Self {
            msg_type: msg_type.into(),
            sequence,
            timestamp_ns: current_timestamp_ns(),
            source: source.to_string(),
            payload: bincode::serialize(payload)?,
        })
    }

    /// Create a wire message with raw payload
    pub fn with_raw_payload(
        msg_type: MessageType,
        sequence: u64,
        source: &str,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            msg_type: msg_type.into(),
            sequence,
            timestamp_ns: current_timestamp_ns(),
            source: source.to_string(),
            payload,
        }
    }

    /// Decode the payload into a typed message
    pub fn decode_payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, bincode::Error> {
        bincode::deserialize(&self.payload)
    }

    /// Get the message type as enum
    pub fn message_type(&self) -> MessageType {
        MessageType::from(self.msg_type)
    }

    /// Serialize the entire wire message
    pub fn serialize(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize a wire message
    pub fn deserialize(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

/// Get current timestamp in nanoseconds
fn current_timestamp_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestPayload {
        value: i32,
        name: String,
    }

    #[test]
    fn test_wire_message_roundtrip() {
        let payload = TestPayload {
            value: 42,
            name: "test".to_string(),
        };

        let msg = WireMessage::new(MessageType::Signal, 1, "test-source", &payload).unwrap();

        // Serialize entire message
        let serialized = msg.serialize().unwrap();

        // Deserialize
        let deserialized = WireMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.msg_type, MSG_SIGNAL);
        assert_eq!(deserialized.sequence, 1);
        assert_eq!(deserialized.source, "test-source");

        // Decode payload
        let decoded: TestPayload = deserialized.decode_payload().unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_message_type_conversion() {
        assert_eq!(MessageType::from(1), MessageType::OrderBookSnapshot);
        assert_eq!(MessageType::from(2), MessageType::DepthUpdate);
        assert_eq!(MessageType::from(3), MessageType::Trade);
        assert_eq!(MessageType::from(4), MessageType::Signal);
        assert_eq!(MessageType::from(5), MessageType::SnapshotRequest);

        assert_eq!(u8::from(MessageType::OrderBookSnapshot), 1);
        assert_eq!(u8::from(MessageType::Signal), 4);
    }
}
