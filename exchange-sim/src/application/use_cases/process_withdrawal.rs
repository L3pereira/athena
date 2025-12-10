//! Process Withdrawal Use Case
//!
//! Handles processing pending withdrawals and updating their status.
//! This would typically be called by a background task or admin action.

use crate::application::ports::{
    AccountRepository, CustodianReader, CustodianWriter, EventPublisher, WithdrawalReader,
    WithdrawalWriter,
};
use crate::domain::{Clock, ExchangeEvent, WithdrawalId, WithdrawalStatus, WithdrawalStatusEvent};
use std::sync::Arc;

/// Command to process a specific withdrawal
#[derive(Debug, Clone)]
pub struct ProcessWithdrawalCommand {
    pub withdrawal_id: WithdrawalId,
}

/// Command to confirm a withdrawal transaction
#[derive(Debug, Clone)]
pub struct ConfirmWithdrawalCommand {
    pub withdrawal_id: WithdrawalId,
    pub tx_hash: String,
}

/// Command to add a confirmation to a withdrawal
#[derive(Debug, Clone)]
pub struct AddConfirmationCommand {
    pub withdrawal_id: WithdrawalId,
}

/// Command to fail a withdrawal
#[derive(Debug, Clone)]
pub struct FailWithdrawalCommand {
    pub withdrawal_id: WithdrawalId,
    pub reason: String,
}

/// Result of processing a withdrawal
#[derive(Debug, Clone)]
pub struct ProcessWithdrawalResult {
    pub withdrawal_id: WithdrawalId,
    pub status: WithdrawalStatus,
}

/// Use case for processing withdrawals
pub struct ProcessWithdrawalUseCase<C, A, W, CR, E>
where
    C: Clock,
    A: AccountRepository,
    W: WithdrawalReader + WithdrawalWriter,
    CR: CustodianReader + CustodianWriter,
    E: EventPublisher,
{
    clock: Arc<C>,
    account_repo: Arc<A>,
    withdrawal_repo: Arc<W>,
    custodian_repo: Arc<CR>,
    event_publisher: Arc<E>,
}

impl<C, A, W, CR, E> ProcessWithdrawalUseCase<C, A, W, CR, E>
where
    C: Clock,
    A: AccountRepository,
    W: WithdrawalReader + WithdrawalWriter,
    CR: CustodianReader + CustodianWriter,
    E: EventPublisher,
{
    pub fn new(
        clock: Arc<C>,
        account_repo: Arc<A>,
        withdrawal_repo: Arc<W>,
        custodian_repo: Arc<CR>,
        event_publisher: Arc<E>,
    ) -> Self {
        Self {
            clock,
            account_repo,
            withdrawal_repo,
            custodian_repo,
            event_publisher,
        }
    }

    /// Start processing a pending withdrawal
    pub async fn start_processing(
        &self,
        command: ProcessWithdrawalCommand,
    ) -> Result<ProcessWithdrawalResult, ProcessWithdrawalError> {
        let mut withdrawal = self
            .withdrawal_repo
            .get(&command.withdrawal_id)
            .await
            .ok_or(ProcessWithdrawalError::WithdrawalNotFound)?;

        let custodian_id = withdrawal
            .custodian_id
            .ok_or(ProcessWithdrawalError::NoCustodianAssigned)?;

        withdrawal
            .start_processing(custodian_id)
            .map_err(|e| ProcessWithdrawalError::InvalidState(e.to_string()))?;

        self.withdrawal_repo.save(withdrawal.clone()).await;

        // Publish status update
        let event = WithdrawalStatusEvent::from_withdrawal(&withdrawal, self.clock.now_millis());
        self.event_publisher
            .publish(ExchangeEvent::WithdrawalStatus(event))
            .await;

        Ok(ProcessWithdrawalResult {
            withdrawal_id: withdrawal.id,
            status: withdrawal.status,
        })
    }

    /// Submit a transaction for a withdrawal
    pub async fn submit_transaction(
        &self,
        command: ConfirmWithdrawalCommand,
    ) -> Result<ProcessWithdrawalResult, ProcessWithdrawalError> {
        let mut withdrawal = self
            .withdrawal_repo
            .get(&command.withdrawal_id)
            .await
            .ok_or(ProcessWithdrawalError::WithdrawalNotFound)?;

        withdrawal
            .submit_transaction(&command.tx_hash)
            .map_err(|e| ProcessWithdrawalError::InvalidState(e.to_string()))?;

        self.withdrawal_repo.save(withdrawal.clone()).await;

        // Publish status update
        let event = WithdrawalStatusEvent::from_withdrawal(&withdrawal, self.clock.now_millis());
        self.event_publisher
            .publish(ExchangeEvent::WithdrawalStatus(event))
            .await;

        Ok(ProcessWithdrawalResult {
            withdrawal_id: withdrawal.id,
            status: withdrawal.status,
        })
    }

    /// Add a confirmation to a withdrawal (may complete it)
    pub async fn add_confirmation(
        &self,
        command: AddConfirmationCommand,
    ) -> Result<ProcessWithdrawalResult, ProcessWithdrawalError> {
        let mut withdrawal = self
            .withdrawal_repo
            .get(&command.withdrawal_id)
            .await
            .ok_or(ProcessWithdrawalError::WithdrawalNotFound)?;

        let was_completed = withdrawal.status == WithdrawalStatus::Completed;
        withdrawal.add_confirmation();
        let is_completed = withdrawal.status == WithdrawalStatus::Completed;

        // If withdrawal just completed, finalize the account changes
        if !was_completed && is_completed {
            self.finalize_withdrawal(&withdrawal).await?;
        }

        self.withdrawal_repo.save(withdrawal.clone()).await;

        // Publish status update
        let event = WithdrawalStatusEvent::from_withdrawal(&withdrawal, self.clock.now_millis());
        self.event_publisher
            .publish(ExchangeEvent::WithdrawalStatus(event))
            .await;

        Ok(ProcessWithdrawalResult {
            withdrawal_id: withdrawal.id,
            status: withdrawal.status,
        })
    }

    /// Fail a withdrawal
    pub async fn fail(
        &self,
        command: FailWithdrawalCommand,
    ) -> Result<ProcessWithdrawalResult, ProcessWithdrawalError> {
        let mut withdrawal = self
            .withdrawal_repo
            .get(&command.withdrawal_id)
            .await
            .ok_or(ProcessWithdrawalError::WithdrawalNotFound)?;

        withdrawal.fail(&command.reason);

        // Return funds to account
        self.refund_withdrawal(&withdrawal).await?;

        self.withdrawal_repo.save(withdrawal.clone()).await;

        // Publish status update
        let event = WithdrawalStatusEvent::from_withdrawal(&withdrawal, self.clock.now_millis());
        self.event_publisher
            .publish(ExchangeEvent::WithdrawalStatus(event))
            .await;

        Ok(ProcessWithdrawalResult {
            withdrawal_id: withdrawal.id,
            status: withdrawal.status,
        })
    }

    /// Cancel a pending withdrawal
    pub async fn cancel(
        &self,
        withdrawal_id: &WithdrawalId,
    ) -> Result<ProcessWithdrawalResult, ProcessWithdrawalError> {
        let mut withdrawal = self
            .withdrawal_repo
            .get(withdrawal_id)
            .await
            .ok_or(ProcessWithdrawalError::WithdrawalNotFound)?;

        withdrawal
            .cancel()
            .map_err(|e| ProcessWithdrawalError::InvalidState(e.to_string()))?;

        // Return funds to account
        self.refund_withdrawal(&withdrawal).await?;

        self.withdrawal_repo.save(withdrawal.clone()).await;

        // Publish status update
        let event = WithdrawalStatusEvent::from_withdrawal(&withdrawal, self.clock.now_millis());
        self.event_publisher
            .publish(ExchangeEvent::WithdrawalStatus(event))
            .await;

        Ok(ProcessWithdrawalResult {
            withdrawal_id: withdrawal.id,
            status: withdrawal.status,
        })
    }

    /// Finalize a completed withdrawal - deduct from account and custodian
    async fn finalize_withdrawal(
        &self,
        withdrawal: &crate::domain::WithdrawalRequest,
    ) -> Result<(), ProcessWithdrawalError> {
        // Get account
        let mut account = self
            .account_repo
            .get(withdrawal.account_id)
            .await
            .ok_or(ProcessWithdrawalError::AccountNotFound)?;

        // Unlock and withdraw the funds
        let total = withdrawal.total_amount();
        account.unlock(&withdrawal.asset, total);
        account
            .withdraw(&withdrawal.asset, total)
            .map_err(|e| ProcessWithdrawalError::AccountError(e.to_string()))?;

        self.account_repo.save(account).await;

        // Deduct from custodian
        if let Some(custodian_id) = withdrawal.custodian_id
            && let Some(mut custodian) = self.custodian_repo.get(&custodian_id).await
        {
            custodian
                .withdraw(&withdrawal.asset, withdrawal.amount)
                .map_err(|e| ProcessWithdrawalError::CustodianError(e.to_string()))?;
            self.custodian_repo.save(custodian).await;
        }

        Ok(())
    }

    /// Refund a failed/cancelled withdrawal
    async fn refund_withdrawal(
        &self,
        withdrawal: &crate::domain::WithdrawalRequest,
    ) -> Result<(), ProcessWithdrawalError> {
        // Get account and unlock funds
        let mut account = self
            .account_repo
            .get(withdrawal.account_id)
            .await
            .ok_or(ProcessWithdrawalError::AccountNotFound)?;

        let total = withdrawal.total_amount();
        account.unlock(&withdrawal.asset, total);

        self.account_repo.save(account).await;

        Ok(())
    }

    /// Process all pending withdrawals (batch processing)
    pub async fn process_pending(
        &self,
    ) -> Vec<Result<ProcessWithdrawalResult, ProcessWithdrawalError>> {
        let pending = self.withdrawal_repo.get_pending().await;
        let mut results = Vec::new();

        for withdrawal in pending {
            let result = self
                .start_processing(ProcessWithdrawalCommand {
                    withdrawal_id: withdrawal.id,
                })
                .await;
            results.push(result);
        }

        results
    }
}

/// Errors that can occur during withdrawal processing
#[derive(Debug, Clone)]
pub enum ProcessWithdrawalError {
    WithdrawalNotFound,
    AccountNotFound,
    NoCustodianAssigned,
    InvalidState(String),
    AccountError(String),
    CustodianError(String),
}

impl std::fmt::Display for ProcessWithdrawalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessWithdrawalError::WithdrawalNotFound => write!(f, "Withdrawal not found"),
            ProcessWithdrawalError::AccountNotFound => write!(f, "Account not found"),
            ProcessWithdrawalError::NoCustodianAssigned => write!(f, "No custodian assigned"),
            ProcessWithdrawalError::InvalidState(s) => write!(f, "Invalid state: {}", s),
            ProcessWithdrawalError::AccountError(s) => write!(f, "Account error: {}", s),
            ProcessWithdrawalError::CustodianError(s) => write!(f, "Custodian error: {}", s),
        }
    }
}

impl std::error::Error for ProcessWithdrawalError {}
