//! Infrastructure adapter for blockchain ports.
//!
//! Wraps the BlockchainSimulator to implement the port traits,
//! following the Adapter pattern from Clean Architecture.

use crate::application::ports::{
    DepositAddressGenerator, DepositAddressRegistry, DepositScanner, ProcessedDepositTracker,
};
use crate::domain::entities::Network;
use crate::domain::services::{BlockchainSimulator, BlockchainTx, DepositAddress, TxId};
use crate::domain::value_objects::Timestamp;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// BLOCKCHAIN ADAPTER
// ============================================================================

/// Adapter that implements blockchain ports by wrapping BlockchainSimulator.
///
/// This follows the Adapter pattern - it adapts the BlockchainSimulator's
/// interface to match the port traits expected by use cases.
#[derive(Clone)]
pub struct BlockchainAdapter {
    simulator: Arc<RwLock<BlockchainSimulator>>,
}

impl BlockchainAdapter {
    pub fn new(simulator: Arc<RwLock<BlockchainSimulator>>) -> Self {
        Self { simulator }
    }

    /// Get direct access to the underlying simulator (for tests/setup)
    pub fn simulator(&self) -> &Arc<RwLock<BlockchainSimulator>> {
        &self.simulator
    }
}

/// Error type for blockchain operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockchainAdapterError(pub String);

impl std::fmt::Display for BlockchainAdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Blockchain error: {}", self.0)
    }
}

impl std::error::Error for BlockchainAdapterError {}

#[async_trait]
impl DepositAddressGenerator for BlockchainAdapter {
    type Error = BlockchainAdapterError;

    async fn generate_address(
        &self,
        network: Network,
        owner_id: &str,
        asset: Option<String>,
        now: Timestamp,
    ) -> Result<DepositAddress, Self::Error> {
        let mut sim = self.simulator.write().await;
        sim.create_deposit_address(network, owner_id, asset, now)
            .map_err(|e| BlockchainAdapterError(e.to_string()))
    }
}

#[async_trait]
impl DepositScanner for BlockchainAdapter {
    async fn get_finalized_deposits(&self, address: &str) -> Vec<BlockchainTx> {
        let sim = self.simulator.read().await;
        // Clone the transactions since we need to return owned values
        sim.get_deposits(address).into_iter().cloned().collect()
    }
}

// ============================================================================
// IN-MEMORY DEPOSIT ADDRESS REGISTRY
// ============================================================================

/// In-memory implementation of deposit address registry.
///
/// For production, this would be backed by a database.
#[derive(Default)]
pub struct InMemoryDepositAddressRegistry {
    addresses: RwLock<HashMap<String, String>>,
}

impl InMemoryDepositAddressRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DepositAddressRegistry for InMemoryDepositAddressRegistry {
    async fn register(&self, address: String, owner_id: String) {
        let mut addresses = self.addresses.write().await;
        addresses.insert(address, owner_id);
    }

    async fn get_owner(&self, address: &str) -> Option<String> {
        let addresses = self.addresses.read().await;
        addresses.get(address).cloned()
    }

    async fn get_all(&self) -> Vec<(String, String)> {
        let addresses = self.addresses.read().await;
        addresses
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

// ============================================================================
// IN-MEMORY PROCESSED DEPOSIT TRACKER
// ============================================================================

/// In-memory implementation of processed deposit tracker.
///
/// For production, this would be backed by a database to ensure
/// idempotency survives restarts.
#[derive(Default)]
pub struct InMemoryProcessedDepositTracker {
    processed: RwLock<HashSet<TxId>>,
}

impl InMemoryProcessedDepositTracker {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ProcessedDepositTracker for InMemoryProcessedDepositTracker {
    async fn is_processed(&self, tx_id: &TxId) -> bool {
        let processed = self.processed.read().await;
        processed.contains(tx_id)
    }

    async fn mark_processed(&self, tx_id: TxId) {
        let mut processed = self.processed.write().await;
        processed.insert(tx_id);
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_deposit_address_registry() {
        let registry = InMemoryDepositAddressRegistry::new();

        registry
            .register("0xabc123".to_string(), "user1".to_string())
            .await;
        registry
            .register("0xdef456".to_string(), "user2".to_string())
            .await;

        assert_eq!(
            registry.get_owner("0xabc123").await,
            Some("user1".to_string())
        );
        assert_eq!(
            registry.get_owner("0xdef456").await,
            Some("user2".to_string())
        );
        assert_eq!(registry.get_owner("0xunknown").await, None);

        let all = registry.get_all().await;
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_processed_deposit_tracker() {
        let tracker = InMemoryProcessedDepositTracker::new();
        let tx_id = TxId::new();

        assert!(!tracker.is_processed(&tx_id).await);

        tracker.mark_processed(tx_id).await;

        assert!(tracker.is_processed(&tx_id).await);
    }

    #[tokio::test]
    async fn test_blockchain_adapter_generate_address() {
        let sim = Arc::new(RwLock::new(BlockchainSimulator::new(Utc::now())));
        let adapter = BlockchainAdapter::new(sim);

        let address = adapter
            .generate_address(
                Network::Ethereum,
                "user1",
                Some("ETH".to_string()),
                Utc::now(),
            )
            .await
            .unwrap();

        assert!(address.address.starts_with("0x"));
        assert_eq!(address.owner_id, "user1");
    }
}
