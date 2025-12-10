use async_trait::async_trait;
use trading_core::DepthSnapshotEvent;

use super::events::StreamData;
use super::exchange::{ExchangeId, QualifiedSymbol};
use crate::gateway_in::infrastructure::RestError;

/// Trait for fetching order book depth snapshots
/// Implements Interface Segregation - only depth fetching capability
#[async_trait]
pub trait DepthFetcher: Send + Sync {
    async fn get_depth(
        &self,
        symbol: &str,
        limit: Option<u32>,
    ) -> Result<DepthSnapshotEvent, RestError>;
}

/// Trait for writing to order books
/// Implements Dependency Inversion - handlers depend on this abstraction
pub trait OrderBookWriter: Send + Sync {
    /// Apply a full snapshot, replacing the current book state
    fn apply_snapshot(&self, key: &QualifiedSymbol, snapshot: &DepthSnapshotEvent);

    /// Apply an incremental update. Returns false if out of sync.
    fn apply_update(&self, exchange_id: &ExchangeId, update: &StreamData) -> bool;
}

/// Trait for parsing stream data
/// Implements Open/Closed - add new parsers without modifying existing code
pub trait StreamParser: Send + Sync {
    /// Check if this parser can handle the given stream name
    fn can_parse(&self, stream: &str) -> bool;

    /// Parse the stream data. Returns None if parsing fails.
    fn parse(&self, stream: &str, data: &serde_json::Value) -> Option<StreamData>;
}
