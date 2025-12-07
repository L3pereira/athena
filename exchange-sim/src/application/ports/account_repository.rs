use crate::domain::{Account, AccountId};
use async_trait::async_trait;

/// Repository for managing trading accounts
///
/// This port abstracts the storage and retrieval of accounts,
/// enabling balance checks, margin tracking, and position management.
#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// Get an account by ID
    async fn get(&self, id: AccountId) -> Option<Account>;

    /// Get an account by owner ID
    async fn get_by_owner(&self, owner_id: &str) -> Option<Account>;

    /// Save an account (insert or update)
    async fn save(&self, account: Account);

    /// Get or create an account for an owner
    async fn get_or_create(&self, owner_id: &str) -> Account;

    /// Check if an account exists
    async fn exists(&self, id: AccountId) -> bool;

    /// Get all accounts
    async fn list(&self) -> Vec<Account>;
}
