use async_trait::async_trait;
use std::fmt;
use trading_core::DepthSnapshotEvent;

use super::events::StreamData;
use super::exchange::{ExchangeId, QualifiedSymbol};

/// Domain error for depth fetching operations
///
/// This is a domain-level abstraction that doesn't expose infrastructure details.
/// Infrastructure implementations convert their specific errors to this type.
#[derive(Debug)]
pub enum FetchError {
    /// Network or communication failure
    Network(String),
    /// API returned an error response
    Api { code: i32, message: String },
    /// Failed to parse the response
    Parse(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Network(msg) => write!(f, "Network error: {}", msg),
            FetchError::Api { code, message } => write!(f, "API error {}: {}", code, message),
            FetchError::Parse(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for FetchError {}

/// Trait for fetching order book depth snapshots
///
/// Implements Interface Segregation - only depth fetching capability.
/// Uses domain-level FetchError to avoid infrastructure leakage.
#[async_trait]
pub trait DepthFetcher: Send + Sync {
    async fn get_depth(
        &self,
        symbol: &str,
        limit: Option<u32>,
    ) -> Result<DepthSnapshotEvent, FetchError>;
}

/// Trait for applying full order book snapshots
///
/// Implements Interface Segregation - separated from update operations.
pub trait SnapshotWriter: Send + Sync {
    /// Apply a full snapshot, replacing the current book state
    fn apply_snapshot(&self, key: &QualifiedSymbol, snapshot: &DepthSnapshotEvent);
}

/// Trait for applying incremental order book updates
///
/// Implements Interface Segregation - separated from snapshot operations.
pub trait UpdateWriter: Send + Sync {
    /// Apply an incremental update. Returns false if out of sync.
    fn apply_update(&self, exchange_id: &ExchangeId, update: &StreamData) -> bool;
}

/// Combined trait for writing to order books (backward compatibility)
///
/// Implements Dependency Inversion - handlers depend on this abstraction.
/// This is a supertrait combining SnapshotWriter and UpdateWriter for
/// cases where both capabilities are needed together.
pub trait OrderBookWriter: SnapshotWriter + UpdateWriter {}

// Blanket implementation: anything implementing both traits is an OrderBookWriter
impl<T: SnapshotWriter + UpdateWriter> OrderBookWriter for T {}

/// Trait for parsing stream data
///
/// Implements Open/Closed - add new parsers without modifying existing code.
pub trait StreamParser: Send + Sync {
    /// Check if this parser can handle the given stream name
    fn can_parse(&self, stream: &str) -> bool;

    /// Parse the stream data. Returns None if parsing fails.
    fn parse(&self, stream: &str, data: &serde_json::Value) -> Option<StreamData>;
}
