use crate::application::ports::AccountRepository;
use crate::domain::{Account, AccountId};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

/// In-memory account repository
///
/// Thread-safe storage for accounts using DashMap.
/// Suitable for simulation and testing.
pub struct InMemoryAccountRepository {
    /// Accounts by ID
    accounts: Arc<DashMap<AccountId, Account>>,
    /// Index: owner_id -> account_id
    owner_index: Arc<DashMap<String, AccountId>>,
}

impl InMemoryAccountRepository {
    pub fn new() -> Self {
        Self {
            accounts: Arc::new(DashMap::new()),
            owner_index: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryAccountRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InMemoryAccountRepository {
    fn clone(&self) -> Self {
        Self {
            accounts: Arc::clone(&self.accounts),
            owner_index: Arc::clone(&self.owner_index),
        }
    }
}

#[async_trait]
impl AccountRepository for InMemoryAccountRepository {
    async fn get(&self, id: AccountId) -> Option<Account> {
        self.accounts.get(&id).map(|a| a.value().clone())
    }

    async fn get_by_owner(&self, owner_id: &str) -> Option<Account> {
        let account_id = self.owner_index.get(owner_id)?;
        self.accounts
            .get(account_id.value())
            .map(|a| a.value().clone())
    }

    async fn save(&self, account: Account) {
        let id = account.id;
        let owner = account.owner_id.clone();
        self.owner_index.insert(owner, id);
        self.accounts.insert(id, account);
    }

    async fn get_or_create(&self, owner_id: &str) -> Account {
        // Check existing
        if let Some(account_id) = self.owner_index.get(owner_id) {
            if let Some(account) = self.accounts.get(account_id.value()) {
                return account.value().clone();
            }
        }

        // Create new
        let account = Account::new(owner_id);
        let id = account.id;
        self.owner_index.insert(owner_id.to_string(), id);
        self.accounts.insert(id, account.clone());
        account
    }

    async fn exists(&self, id: AccountId) -> bool {
        self.accounts.contains_key(&id)
    }

    async fn list(&self) -> Vec<Account> {
        self.accounts.iter().map(|e| e.value().clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[tokio::test]
    async fn test_get_or_create() {
        let repo = InMemoryAccountRepository::new();

        let account1 = repo.get_or_create("user1").await;
        let account2 = repo.get_or_create("user1").await;

        assert_eq!(account1.id, account2.id);
        assert_eq!(account1.owner_id, "user1");
    }

    #[tokio::test]
    async fn test_save_and_get() {
        let repo = InMemoryAccountRepository::new();

        let mut account = Account::new("user1");
        account.deposit("USDT", dec!(10000));
        let id = account.id;

        repo.save(account).await;

        let retrieved = repo.get(id).await.unwrap();
        assert_eq!(retrieved.balance("USDT").available, dec!(10000));
    }

    #[tokio::test]
    async fn test_get_by_owner() {
        let repo = InMemoryAccountRepository::new();

        let account = repo.get_or_create("trader123").await;
        let id = account.id;

        let retrieved = repo.get_by_owner("trader123").await.unwrap();
        assert_eq!(retrieved.id, id);
    }
}
