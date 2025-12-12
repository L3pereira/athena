//! Transport Error Types

use thiserror::Error;

/// Error type for transport operations
#[derive(Error, Debug, Clone)]
pub enum TransportError {
    /// Channel/connection is closed
    #[error("channel closed")]
    ChannelClosed,

    /// Buffer is full (backpressure)
    #[error("buffer full")]
    Full,

    /// No messages available (non-blocking poll)
    #[error("empty")]
    Empty,

    /// IO error
    #[error("IO error: {0}")]
    Io(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Configuration error
    #[error("configuration error: {0}")]
    Config(String),

    /// Connection failed
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// Timeout
    #[error("timeout")]
    Timeout,
}

impl From<std::io::Error> for TransportError {
    fn from(err: std::io::Error) -> Self {
        TransportError::Io(err.to_string())
    }
}

impl From<bincode::Error> for TransportError {
    fn from(err: bincode::Error) -> Self {
        TransportError::Serialization(err.to_string())
    }
}
