//! Error types for the gateway crate

use thiserror::Error;

/// Transport-level errors
#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Subscription failed: {0}")]
    Subscribe(String),

    #[error("Send failed: {0}")]
    Send(String),

    #[error("Request failed: {0}")]
    Request(String),

    #[error("Serialization failed: {0}")]
    Serialization(String),

    #[error("Deserialization failed: {0}")]
    Deserialization(String),

    #[error("Channel closed")]
    ChannelClosed,

    #[error("Invalid transport mode for this operation")]
    InvalidMode,

    #[error("Timeout waiting for response")]
    Timeout,
}

/// Gateway-level errors (adapter operations)
#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Invalid mode for operation")]
    InvalidMode,

    #[error("Message conversion error: {0}")]
    Conversion(String),

    #[error("Exchange error: {0}")]
    Exchange(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<bincode::Error> for GatewayError {
    fn from(e: bincode::Error) -> Self {
        GatewayError::Serialization(e.to_string())
    }
}
