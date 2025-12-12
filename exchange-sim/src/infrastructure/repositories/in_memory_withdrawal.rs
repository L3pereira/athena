//! In-memory withdrawal repository implementation

use crate::application::ports::{WithdrawalReader, WithdrawalWriter};
use crate::domain::{AccountId, WithdrawalId, WithdrawalRequest, WithdrawalStatus};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

/// In-memory withdrawal repository
///
/// Thread-safe storage for withdrawals using DashMap.
pub struct InMemoryWithdrawalRepository {
    withdrawals: Arc<DashMap<WithdrawalId, WithdrawalRequest>>,
}

impl InMemoryWithdrawalRepository {
    pub fn new() -> Self {
        Self {
            withdrawals: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryWithdrawalRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InMemoryWithdrawalRepository {
    fn clone(&self) -> Self {
        Self {
            withdrawals: Arc::clone(&self.withdrawals),
        }
    }
}

#[async_trait]
impl WithdrawalReader for InMemoryWithdrawalRepository {
    async fn get(&self, id: &WithdrawalId) -> Option<WithdrawalRequest> {
        self.withdrawals.get(id).map(|w| w.value().clone())
    }

    async fn get_by_account(&self, account_id: &AccountId) -> Vec<WithdrawalRequest> {
        self.withdrawals
            .iter()
            .filter(|w| &w.account_id == account_id)
            .map(|w| w.value().clone())
            .collect()
    }

    async fn get_by_status(&self, status: WithdrawalStatus) -> Vec<WithdrawalRequest> {
        self.withdrawals
            .iter()
            .filter(|w| w.status == status)
            .map(|w| w.value().clone())
            .collect()
    }

    async fn get_pending(&self) -> Vec<WithdrawalRequest> {
        self.get_by_status(WithdrawalStatus::Pending).await
    }

    async fn get_awaiting_confirmation(&self) -> Vec<WithdrawalRequest> {
        self.get_by_status(WithdrawalStatus::AwaitingConfirmation)
            .await
    }
}

#[async_trait]
impl WithdrawalWriter for InMemoryWithdrawalRepository {
    async fn save(&self, withdrawal: WithdrawalRequest) {
        self.withdrawals.insert(withdrawal.id, withdrawal);
    }

    async fn delete(&self, id: &WithdrawalId) -> bool {
        self.withdrawals.remove(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Network, Value};
    use uuid::Uuid;

    fn make_withdrawal(account_id: AccountId) -> WithdrawalRequest {
        WithdrawalRequest::new(
            account_id,
            "USDT",
            Value::from_int(100),
            Value::from_int(5),
            Network::Ethereum,
            "0x1234567890abcdef1234567890abcdef12345678",
        )
    }

    #[tokio::test]
    async fn test_save_and_get() {
        let repo = InMemoryWithdrawalRepository::new();
        let account_id = Uuid::new_v4();

        let withdrawal = make_withdrawal(account_id);
        let id = withdrawal.id;

        repo.save(withdrawal).await;

        let retrieved = repo.get(&id).await.unwrap();
        assert_eq!(retrieved.amount, Value::from_int(100));
    }

    #[tokio::test]
    async fn test_get_by_account() {
        let repo = InMemoryWithdrawalRepository::new();
        let account1 = Uuid::new_v4();
        let account2 = Uuid::new_v4();

        repo.save(make_withdrawal(account1)).await;
        repo.save(make_withdrawal(account1)).await;
        repo.save(make_withdrawal(account2)).await;

        let acc1_withdrawals = repo.get_by_account(&account1).await;
        assert_eq!(acc1_withdrawals.len(), 2);

        let acc2_withdrawals = repo.get_by_account(&account2).await;
        assert_eq!(acc2_withdrawals.len(), 1);
    }

    #[tokio::test]
    async fn test_get_pending() {
        let repo = InMemoryWithdrawalRepository::new();
        let account_id = Uuid::new_v4();

        let mut withdrawal = make_withdrawal(account_id);
        repo.save(withdrawal.clone()).await;

        let pending = repo.get_pending().await;
        assert_eq!(pending.len(), 1);

        // Mark as processing
        withdrawal
            .start_processing(crate::domain::CustodianId::new())
            .ok();
        repo.save(withdrawal).await;

        let pending = repo.get_pending().await;
        assert_eq!(pending.len(), 0);
    }
}
