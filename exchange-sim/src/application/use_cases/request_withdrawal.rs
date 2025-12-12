//! Request Withdrawal Use Case
//!
//! Handles initiating a new withdrawal request from user account.

use crate::application::ports::{
    AccountRepository, CustodianReader, EventPublisher, WithdrawalWriter,
};
use crate::domain::{
    Clock, ExchangeEvent, Network, Value, WithdrawalError, WithdrawalRequest, WithdrawalStatusEvent,
};
use std::sync::Arc;

/// Command to request a withdrawal
#[derive(Debug, Clone)]
pub struct RequestWithdrawalCommand {
    pub asset: String,
    pub amount: Value,
    pub network: Network,
    pub destination_address: String,
    pub memo: Option<String>,
}

/// Result of requesting a withdrawal
#[derive(Debug, Clone)]
pub struct RequestWithdrawalResult {
    pub withdrawal: WithdrawalRequest,
    pub fee: Value,
    pub estimated_time_secs: u64,
}

/// Use case for requesting withdrawals
pub struct RequestWithdrawalUseCase<C, A, W, CR, E>
where
    C: Clock,
    A: AccountRepository,
    W: WithdrawalWriter,
    CR: CustodianReader,
    E: EventPublisher,
{
    clock: Arc<C>,
    account_repo: Arc<A>,
    withdrawal_repo: Arc<W>,
    custodian_repo: Arc<CR>,
    event_publisher: Arc<E>,
}

impl<C, A, W, CR, E> RequestWithdrawalUseCase<C, A, W, CR, E>
where
    C: Clock,
    A: AccountRepository,
    W: WithdrawalWriter,
    CR: CustodianReader,
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

    pub async fn execute(
        &self,
        client_id: &str,
        command: RequestWithdrawalCommand,
    ) -> Result<RequestWithdrawalResult, WithdrawalUseCaseError> {
        // Get account
        let mut account = self
            .account_repo
            .get_by_owner(client_id)
            .await
            .ok_or(WithdrawalUseCaseError::AccountNotFound)?;

        // Find an active custodian supporting this asset and network
        let custodians = self.custodian_repo.get_by_network(&command.network).await;
        let custodian = custodians
            .iter()
            .find(|c| c.active && c.supports_withdrawal(&command.asset))
            .ok_or_else(|| WithdrawalUseCaseError::NoCustodianAvailable {
                asset: command.asset.clone(),
                network: command.network.clone(),
            })?;

        // Get withdrawal config
        let config = custodian.withdrawal_config(&command.asset).ok_or(
            WithdrawalUseCaseError::WithdrawalError(WithdrawalError::AssetNotSupported(
                command.asset.clone(),
            )),
        )?;

        // Validate amount
        config
            .validate_amount(command.amount)
            .map_err(WithdrawalUseCaseError::WithdrawalError)?;

        // Calculate total (amount + fee)
        let total = config.total_required(command.amount);

        // Check account balance
        let balance = account.balance(&command.asset);
        if balance.available.raw() < total.raw() {
            return Err(WithdrawalUseCaseError::WithdrawalError(
                WithdrawalError::InsufficientBalance {
                    available: balance.available,
                    requested: total,
                },
            ));
        }

        // Lock the funds
        account.lock(&command.asset, total).map_err(|_| {
            WithdrawalUseCaseError::WithdrawalError(WithdrawalError::InsufficientBalance {
                available: balance.available,
                requested: total,
            })
        })?;

        // Create withdrawal request
        let mut withdrawal = WithdrawalRequest::new(
            account.id,
            &command.asset,
            command.amount,
            config.fee,
            command.network.clone(),
            &command.destination_address,
        )
        .with_custodian(custodian.id)
        .with_confirmations_required(config.confirmations_required);

        if let Some(memo) = command.memo {
            withdrawal = withdrawal.with_memo(memo);
        }

        // Save account and withdrawal
        self.account_repo.save(account).await;
        self.withdrawal_repo.save(withdrawal.clone()).await;

        // Publish event
        let event = WithdrawalStatusEvent::from_withdrawal(&withdrawal, self.clock.now_millis());
        self.event_publisher
            .publish(ExchangeEvent::WithdrawalStatus(event))
            .await;

        Ok(RequestWithdrawalResult {
            withdrawal,
            fee: config.fee,
            estimated_time_secs: config.processing_time_secs,
        })
    }
}

/// Errors that can occur during withdrawal request
#[derive(Debug, Clone)]
pub enum WithdrawalUseCaseError {
    AccountNotFound,
    NoCustodianAvailable { asset: String, network: Network },
    WithdrawalError(WithdrawalError),
    InternalError(String),
}

impl std::fmt::Display for WithdrawalUseCaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WithdrawalUseCaseError::AccountNotFound => write!(f, "Account not found"),
            WithdrawalUseCaseError::NoCustodianAvailable { asset, network } => {
                write!(f, "No custodian available for {} on {}", asset, network)
            }
            WithdrawalUseCaseError::WithdrawalError(e) => write!(f, "{}", e),
            WithdrawalUseCaseError::InternalError(s) => write!(f, "Internal error: {}", s),
        }
    }
}

impl std::error::Error for WithdrawalUseCaseError {}
