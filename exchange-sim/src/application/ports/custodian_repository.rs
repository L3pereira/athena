//! Port for custodian repository operations
//!
//! Follows Interface Segregation Principle with focused traits.

use async_trait::async_trait;

use crate::domain::{Custodian, CustodianId, Network};

/// Read operations for custodians
#[async_trait]
pub trait CustodianReader: Send + Sync {
    /// Get a custodian by ID
    async fn get(&self, id: &CustodianId) -> Option<Custodian>;

    /// Get custodian by network and type
    async fn get_by_network(&self, network: &Network) -> Vec<Custodian>;

    /// Get all active custodians
    async fn get_active(&self) -> Vec<Custodian>;

    /// Get custodians supporting a specific asset
    async fn get_supporting_asset(&self, asset: &str) -> Vec<Custodian>;
}

/// Write operations for custodians
#[async_trait]
pub trait CustodianWriter: Send + Sync {
    /// Save a custodian
    async fn save(&self, custodian: Custodian);

    /// Delete a custodian
    async fn delete(&self, id: &CustodianId) -> bool;
}

/// Combined repository trait
#[async_trait]
pub trait CustodianRepository: CustodianReader + CustodianWriter {}

// Blanket implementation
impl<T: CustodianReader + CustodianWriter> CustodianRepository for T {}
