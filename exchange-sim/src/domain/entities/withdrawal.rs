//! Withdrawal request entity for tracking pending and completed withdrawals
//!
//! Withdrawals go through several states:
//! 1. Pending - Request created, awaiting processing
//! 2. Processing - Being processed by custodian
//! 3. AwaitingConfirmation - Transaction submitted, waiting for confirms
//! 4. Completed - Successfully completed
//! 5. Failed - Failed for some reason
//! 6. Cancelled - Cancelled by user or admin

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::custodian::{CustodianId, Network, WithdrawalError};
use crate::domain::{AccountId, Timestamp};

/// Unique identifier for a withdrawal request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WithdrawalId(Uuid);

impl WithdrawalId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WithdrawalId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WithdrawalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status of a withdrawal request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WithdrawalStatus {
    /// Request created, awaiting processing
    #[default]
    Pending,
    /// Being processed by the system
    Processing,
    /// Transaction submitted, waiting for confirmations
    AwaitingConfirmation,
    /// Successfully completed
    Completed,
    /// Failed
    Failed,
    /// Cancelled
    Cancelled,
}

impl std::fmt::Display for WithdrawalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WithdrawalStatus::Pending => write!(f, "PENDING"),
            WithdrawalStatus::Processing => write!(f, "PROCESSING"),
            WithdrawalStatus::AwaitingConfirmation => write!(f, "AWAITING_CONFIRMATION"),
            WithdrawalStatus::Completed => write!(f, "COMPLETED"),
            WithdrawalStatus::Failed => write!(f, "FAILED"),
            WithdrawalStatus::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

/// A withdrawal request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRequest {
    /// Unique identifier
    pub id: WithdrawalId,
    /// Account making the withdrawal
    pub account_id: AccountId,
    /// Asset being withdrawn
    pub asset: String,
    /// Amount being withdrawn (excluding fee)
    pub amount: Decimal,
    /// Withdrawal fee
    pub fee: Decimal,
    /// Network for withdrawal
    pub network: Network,
    /// Destination address
    pub destination_address: String,
    /// Optional memo/tag (for some chains)
    pub memo: Option<String>,
    /// Custodian processing this withdrawal
    pub custodian_id: Option<CustodianId>,
    /// Current status
    pub status: WithdrawalStatus,
    /// Transaction hash (once submitted)
    pub tx_hash: Option<String>,
    /// Confirmations received
    pub confirmations: u32,
    /// Confirmations required
    pub confirmations_required: u32,
    /// Created timestamp
    pub created_at: Timestamp,
    /// Updated timestamp
    pub updated_at: Timestamp,
    /// Completed timestamp
    pub completed_at: Option<Timestamp>,
    /// Error message if failed
    pub error_message: Option<String>,
}

impl WithdrawalRequest {
    /// Create a new withdrawal request
    pub fn new(
        account_id: AccountId,
        asset: impl Into<String>,
        amount: Decimal,
        fee: Decimal,
        network: Network,
        destination_address: impl Into<String>,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: WithdrawalId::new(),
            account_id,
            asset: asset.into(),
            amount,
            fee,
            network,
            destination_address: destination_address.into(),
            memo: None,
            custodian_id: None,
            status: WithdrawalStatus::Pending,
            tx_hash: None,
            confirmations: 0,
            confirmations_required: 1,
            created_at: now,
            updated_at: now,
            completed_at: None,
            error_message: None,
        }
    }

    pub fn with_memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = Some(memo.into());
        self
    }

    pub fn with_custodian(mut self, custodian_id: CustodianId) -> Self {
        self.custodian_id = Some(custodian_id);
        self
    }

    pub fn with_confirmations_required(mut self, confirms: u32) -> Self {
        self.confirmations_required = confirms;
        self
    }

    /// Total amount deducted from account (amount + fee)
    pub fn total_amount(&self) -> Decimal {
        self.amount + self.fee
    }

    /// Check if the withdrawal is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            WithdrawalStatus::Completed | WithdrawalStatus::Failed | WithdrawalStatus::Cancelled
        )
    }

    /// Check if the withdrawal can be cancelled
    pub fn can_cancel(&self) -> bool {
        matches!(self.status, WithdrawalStatus::Pending)
    }

    /// Start processing the withdrawal
    pub fn start_processing(&mut self, custodian_id: CustodianId) -> Result<(), WithdrawalError> {
        if self.status != WithdrawalStatus::Pending {
            return Err(WithdrawalError::NetworkError(
                "Cannot process non-pending withdrawal".to_string(),
            ));
        }
        self.custodian_id = Some(custodian_id);
        self.status = WithdrawalStatus::Processing;
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Submit the transaction
    pub fn submit_transaction(
        &mut self,
        tx_hash: impl Into<String>,
    ) -> Result<(), WithdrawalError> {
        if self.status != WithdrawalStatus::Processing {
            return Err(WithdrawalError::NetworkError(
                "Cannot submit transaction for non-processing withdrawal".to_string(),
            ));
        }
        self.tx_hash = Some(tx_hash.into());
        self.status = WithdrawalStatus::AwaitingConfirmation;
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Add a confirmation
    pub fn add_confirmation(&mut self) {
        self.confirmations += 1;
        self.updated_at = chrono::Utc::now();

        if self.confirmations >= self.confirmations_required {
            self.complete();
        }
    }

    /// Complete the withdrawal
    pub fn complete(&mut self) {
        self.status = WithdrawalStatus::Completed;
        self.updated_at = chrono::Utc::now();
        self.completed_at = Some(chrono::Utc::now());
    }

    /// Fail the withdrawal
    pub fn fail(&mut self, reason: impl Into<String>) {
        self.status = WithdrawalStatus::Failed;
        self.error_message = Some(reason.into());
        self.updated_at = chrono::Utc::now();
    }

    /// Cancel the withdrawal
    pub fn cancel(&mut self) -> Result<(), WithdrawalError> {
        if !self.can_cancel() {
            return Err(WithdrawalError::NetworkError(
                "Cannot cancel withdrawal in current state".to_string(),
            ));
        }
        self.status = WithdrawalStatus::Cancelled;
        self.updated_at = chrono::Utc::now();
        Ok(())
    }
}

/// Event emitted when withdrawal status changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalStatusEvent {
    pub withdrawal_id: WithdrawalId,
    pub account_id: AccountId,
    pub asset: String,
    pub amount: Decimal,
    pub status: WithdrawalStatus,
    pub tx_hash: Option<String>,
    pub timestamp: i64,
}

impl WithdrawalStatusEvent {
    pub fn from_withdrawal(withdrawal: &WithdrawalRequest, timestamp_ms: i64) -> Self {
        Self {
            withdrawal_id: withdrawal.id,
            account_id: withdrawal.account_id,
            asset: withdrawal.asset.clone(),
            amount: withdrawal.amount,
            status: withdrawal.status,
            tx_hash: withdrawal.tx_hash.clone(),
            timestamp: timestamp_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn make_withdrawal() -> WithdrawalRequest {
        WithdrawalRequest::new(
            Uuid::new_v4(),
            "USDT",
            dec!(100),
            dec!(5),
            Network::Ethereum,
            "0x1234567890abcdef1234567890abcdef12345678",
        )
    }

    #[test]
    fn test_withdrawal_lifecycle() {
        let mut withdrawal = make_withdrawal();
        assert_eq!(withdrawal.status, WithdrawalStatus::Pending);
        assert_eq!(withdrawal.total_amount(), dec!(105));

        // Start processing
        let custodian_id = CustodianId::new();
        withdrawal.start_processing(custodian_id).unwrap();
        assert_eq!(withdrawal.status, WithdrawalStatus::Processing);

        // Submit transaction
        withdrawal.submit_transaction("0xabc123").unwrap();
        assert_eq!(withdrawal.status, WithdrawalStatus::AwaitingConfirmation);

        // Add confirmation
        withdrawal.add_confirmation();
        assert_eq!(withdrawal.status, WithdrawalStatus::Completed);
        assert!(withdrawal.completed_at.is_some());
    }

    #[test]
    fn test_withdrawal_cancellation() {
        let mut withdrawal = make_withdrawal();
        assert!(withdrawal.can_cancel());

        withdrawal.cancel().unwrap();
        assert_eq!(withdrawal.status, WithdrawalStatus::Cancelled);
        assert!(!withdrawal.can_cancel());
    }

    #[test]
    fn test_withdrawal_failure() {
        let mut withdrawal = make_withdrawal();
        withdrawal.start_processing(CustodianId::new()).unwrap();

        withdrawal.fail("Network timeout");
        assert_eq!(withdrawal.status, WithdrawalStatus::Failed);
        assert_eq!(
            withdrawal.error_message,
            Some("Network timeout".to_string())
        );
    }

    #[test]
    fn test_multiple_confirmations() {
        let mut withdrawal = make_withdrawal().with_confirmations_required(3);

        withdrawal.start_processing(CustodianId::new()).unwrap();
        withdrawal.submit_transaction("0xabc123").unwrap();

        withdrawal.add_confirmation();
        assert_eq!(withdrawal.status, WithdrawalStatus::AwaitingConfirmation);
        assert_eq!(withdrawal.confirmations, 1);

        withdrawal.add_confirmation();
        assert_eq!(withdrawal.status, WithdrawalStatus::AwaitingConfirmation);
        assert_eq!(withdrawal.confirmations, 2);

        withdrawal.add_confirmation();
        assert_eq!(withdrawal.status, WithdrawalStatus::Completed);
        assert_eq!(withdrawal.confirmations, 3);
    }
}
