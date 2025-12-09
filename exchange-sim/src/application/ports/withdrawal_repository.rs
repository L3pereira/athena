//! Port for withdrawal repository operations
//!
//! Follows Interface Segregation Principle with focused traits.

use async_trait::async_trait;

use crate::domain::{AccountId, WithdrawalId, WithdrawalRequest, WithdrawalStatus};

/// Read operations for withdrawals
#[async_trait]
pub trait WithdrawalReader: Send + Sync {
    /// Get a withdrawal by ID
    async fn get(&self, id: &WithdrawalId) -> Option<WithdrawalRequest>;

    /// Get all withdrawals for an account
    async fn get_by_account(&self, account_id: &AccountId) -> Vec<WithdrawalRequest>;

    /// Get withdrawals by status
    async fn get_by_status(&self, status: WithdrawalStatus) -> Vec<WithdrawalRequest>;

    /// Get pending withdrawals (ready for processing)
    async fn get_pending(&self) -> Vec<WithdrawalRequest>;

    /// Get withdrawals awaiting confirmation
    async fn get_awaiting_confirmation(&self) -> Vec<WithdrawalRequest>;
}

/// Write operations for withdrawals
#[async_trait]
pub trait WithdrawalWriter: Send + Sync {
    /// Save a withdrawal request
    async fn save(&self, withdrawal: WithdrawalRequest);

    /// Delete a withdrawal (admin only)
    async fn delete(&self, id: &WithdrawalId) -> bool;
}

/// Combined repository trait
#[async_trait]
pub trait WithdrawalRepository: WithdrawalReader + WithdrawalWriter {}

// Blanket implementation
impl<T: WithdrawalReader + WithdrawalWriter> WithdrawalRepository for T {}
