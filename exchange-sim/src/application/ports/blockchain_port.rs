//! Blockchain ports for deposit processing.
//!
//! Following ISP, these traits expose only the blockchain operations
//! needed by specific use cases, rather than the full BlockchainSimulator.

use crate::domain::entities::Network;
use crate::domain::services::{BlockchainTx, DepositAddress};
use crate::domain::value_objects::Timestamp;
use async_trait::async_trait;

// ============================================================================
// DEPOSIT ADDRESS GENERATION
// ============================================================================

/// Port for generating deposit addresses on blockchain networks.
///
/// Separated from deposit scanning per ISP - a use case that only
/// generates addresses doesn't need scanning capabilities.
#[async_trait]
pub trait DepositAddressGenerator: Send + Sync {
    /// Error type for address generation
    type Error: std::error::Error + Send + Sync;

    /// Generate a new deposit address for an owner on a specific network.
    async fn generate_address(
        &self,
        network: Network,
        owner_id: &str,
        asset: Option<String>,
        now: Timestamp,
    ) -> Result<DepositAddress, Self::Error>;
}

// ============================================================================
// DEPOSIT SCANNING
// ============================================================================

/// Port for scanning blockchain for finalized deposits.
///
/// Separated from address generation per ISP - background deposit
/// processors only need to scan, not generate addresses.
#[async_trait]
pub trait DepositScanner: Send + Sync {
    /// Get all finalized deposits to a specific address.
    ///
    /// Returns only transactions that have reached finality
    /// (enough confirmations for the network).
    async fn get_finalized_deposits(&self, address: &str) -> Vec<BlockchainTx>;
}

// ============================================================================
// COMBINED TRAIT
// ============================================================================

/// Combined blockchain port for use cases that need both capabilities.
///
/// Most deposit use cases need both address generation and scanning,
/// so this provides a convenience trait that combines them.
pub trait BlockchainPort: DepositAddressGenerator + DepositScanner {}

// Blanket implementation
impl<T> BlockchainPort for T where T: DepositAddressGenerator + DepositScanner {}

// ============================================================================
// DEPOSIT ADDRESS REGISTRY
// ============================================================================

/// Port for tracking monitored deposit addresses.
///
/// Extracted from the use case to follow SRP - the use case shouldn't
/// be responsible for address storage/persistence.
#[async_trait]
pub trait DepositAddressRegistry: Send + Sync {
    /// Register an address for deposit monitoring.
    async fn register(&self, address: String, owner_id: String);

    /// Get owner ID for a monitored address.
    async fn get_owner(&self, address: &str) -> Option<String>;

    /// Get all monitored addresses with their owners.
    async fn get_all(&self) -> Vec<(String, String)>;
}

// ============================================================================
// PROCESSED DEPOSIT TRACKER
// ============================================================================

/// Port for tracking processed deposits (idempotency).
///
/// Extracted to follow SRP and enable persistence - in production,
/// this would be backed by a database to survive restarts.
#[async_trait]
pub trait ProcessedDepositTracker: Send + Sync {
    /// Check if a transaction has already been processed.
    async fn is_processed(&self, tx_id: &crate::domain::services::TxId) -> bool;

    /// Mark a transaction as processed.
    async fn mark_processed(&self, tx_id: crate::domain::services::TxId);
}
