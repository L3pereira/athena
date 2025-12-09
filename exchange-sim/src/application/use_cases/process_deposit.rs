//! Process deposit use case for incoming crypto funds.
//!
//! This use case monitors the blockchain for finalized deposits
//! and credits user accounts accordingly.
//!
//! ## SOLID Compliance
//!
//! - **SRP**: Use case only orchestrates deposit processing; state management
//!   is delegated to `DepositAddressRegistry` and `ProcessedDepositTracker`.
//! - **DIP**: Depends on abstractions (traits) not concrete implementations.
//! - **ISP**: Uses focused traits (`DepositAddressGenerator`, `DepositScanner`)
//!   rather than a monolithic blockchain interface.

use crate::application::ports::{
    AccountRepository, DepositAddressGenerator, DepositAddressRegistry, DepositScanner,
    EventPublisher, ProcessedDepositTracker,
};
use crate::domain::services::{Clock, TxId};
use crate::domain::value_objects::Timestamp;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// DEPOSIT TYPES
// ============================================================================

/// Unique identifier for a deposit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DepositId(Uuid);

impl DepositId {
    pub fn new() -> Self {
        DepositId(Uuid::new_v4())
    }
}

impl Default for DepositId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DepositId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status of a deposit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepositStatus {
    /// Deposit detected, waiting for confirmations
    Pending,
    /// Deposit has enough confirmations
    Confirmed,
    /// Deposit credited to account
    Credited,
}

/// A deposit record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deposit {
    pub id: DepositId,
    pub tx_id: TxId,
    pub owner_id: String,
    pub asset: String,
    pub amount: Decimal,
    pub status: DepositStatus,
    pub deposit_address: String,
    pub detected_at: Timestamp,
    pub credited_at: Option<Timestamp>,
}

// ============================================================================
// COMMANDS AND RESULTS
// ============================================================================

/// Command to register a deposit address for monitoring
#[derive(Debug, Clone)]
pub struct RegisterDepositAddressCommand {
    pub owner_id: String,
    pub address: String,
}

/// Result of processing deposits
#[derive(Debug, Clone)]
pub struct ProcessDepositsResult {
    pub processed_count: usize,
    pub credited_deposits: Vec<Deposit>,
}

// ============================================================================
// USE CASE
// ============================================================================

/// Process deposit use case
///
/// Monitors blockchain for incoming deposits and credits accounts.
///
/// ## Type Parameters
///
/// - `C`: Clock implementation for timestamps
/// - `A`: Account repository for balance management
/// - `E`: Event publisher for domain events
/// - `G`: Deposit address generator (blockchain abstraction)
/// - `S`: Deposit scanner (blockchain abstraction)
/// - `R`: Deposit address registry (state management)
/// - `T`: Processed deposit tracker (idempotency)
pub struct ProcessDepositUseCase<C, A, E, G, S, R, T>
where
    C: Clock,
    A: AccountRepository,
    E: EventPublisher,
    G: DepositAddressGenerator,
    S: DepositScanner,
    R: DepositAddressRegistry,
    T: ProcessedDepositTracker,
{
    clock: Arc<C>,
    pub account_repo: Arc<A>,
    event_publisher: Arc<E>,
    address_generator: Arc<G>,
    deposit_scanner: Arc<S>,
    address_registry: Arc<R>,
    processed_tracker: Arc<T>,
}

impl<C, A, E, G, S, R, T> ProcessDepositUseCase<C, A, E, G, S, R, T>
where
    C: Clock,
    A: AccountRepository,
    E: EventPublisher,
    G: DepositAddressGenerator,
    S: DepositScanner,
    R: DepositAddressRegistry,
    T: ProcessedDepositTracker,
{
    pub fn new(
        clock: Arc<C>,
        account_repo: Arc<A>,
        event_publisher: Arc<E>,
        address_generator: Arc<G>,
        deposit_scanner: Arc<S>,
        address_registry: Arc<R>,
        processed_tracker: Arc<T>,
    ) -> Self {
        Self {
            clock,
            account_repo,
            event_publisher,
            address_generator,
            deposit_scanner,
            address_registry,
            processed_tracker,
        }
    }

    /// Register a deposit address for monitoring
    pub async fn register_address(&self, command: RegisterDepositAddressCommand) {
        self.address_registry
            .register(command.address, command.owner_id)
            .await;
    }

    /// Generate a new deposit address for an owner
    pub async fn generate_deposit_address(
        &self,
        owner_id: &str,
        network: crate::domain::entities::Network,
        asset: Option<String>,
    ) -> Result<crate::domain::services::DepositAddress, ProcessDepositError> {
        let now = self.clock.now();

        let address = self
            .address_generator
            .generate_address(network, owner_id, asset, now)
            .await
            .map_err(|e| ProcessDepositError::BlockchainError(e.to_string()))?;

        // Auto-register for monitoring
        self.address_registry
            .register(address.address.clone(), owner_id.to_string())
            .await;

        Ok(address)
    }

    /// Scan blockchain for new deposits and credit accounts
    pub async fn process_deposits(&self) -> Result<ProcessDepositsResult, ProcessDepositError> {
        let now = self.clock.now();
        let monitored = self.address_registry.get_all().await;

        let mut credited_deposits = Vec::new();

        for (address, owner_id) in monitored {
            let deposits = self.deposit_scanner.get_finalized_deposits(&address).await;

            for tx in deposits {
                // Skip already processed
                if self.processed_tracker.is_processed(&tx.id).await {
                    continue;
                }

                // Credit the account
                let mut account = self.account_repo.get_or_create(&owner_id).await;
                account.deposit(&tx.asset, tx.amount);
                self.account_repo.save(account).await;

                // Record as processed
                self.processed_tracker.mark_processed(tx.id).await;

                // Create deposit record
                let deposit = Deposit {
                    id: DepositId::new(),
                    tx_id: tx.id,
                    owner_id: owner_id.clone(),
                    asset: tx.asset.clone(),
                    amount: tx.amount,
                    status: DepositStatus::Credited,
                    deposit_address: address.clone(),
                    detected_at: tx.confirmed_at.unwrap_or(now),
                    credited_at: Some(now),
                };

                // Publish event (fire-and-forget, drop the future)
                drop(
                    self.event_publisher
                        .publish(crate::domain::ExchangeEvent::DepositCredited(
                            DepositCreditedEvent {
                                deposit_id: deposit.id,
                                owner_id: owner_id.clone(),
                                asset: tx.asset.clone(),
                                amount: tx.amount,
                                tx_id: tx.id,
                                timestamp: now,
                            },
                        )),
                );

                credited_deposits.push(deposit);
            }
        }

        Ok(ProcessDepositsResult {
            processed_count: credited_deposits.len(),
            credited_deposits,
        })
    }

    /// Get deposit status for a transaction
    pub async fn get_deposit_status(&self, tx_id: &TxId) -> Option<DepositStatus> {
        if self.processed_tracker.is_processed(tx_id).await {
            Some(DepositStatus::Credited)
        } else {
            // Could extend to check pending status on blockchain
            None
        }
    }
}

// ============================================================================
// EVENTS
// ============================================================================

/// Event emitted when a deposit is credited
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositCreditedEvent {
    pub deposit_id: DepositId,
    pub owner_id: String,
    pub asset: String,
    pub amount: Decimal,
    pub tx_id: TxId,
    pub timestamp: Timestamp,
}

// ============================================================================
// ERRORS
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessDepositError {
    BlockchainError(String),
    AccountNotFound(String),
}

impl std::fmt::Display for ProcessDepositError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessDepositError::BlockchainError(e) => write!(f, "Blockchain error: {}", e),
            ProcessDepositError::AccountNotFound(id) => write!(f, "Account not found: {}", id),
        }
    }
}

impl std::error::Error for ProcessDepositError {}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::Network;
    use crate::domain::services::{BlockchainSimulator, ControllableClock};
    use crate::infrastructure::{
        BlockchainAdapter, BroadcastEventPublisher, InMemoryAccountRepository,
        InMemoryDepositAddressRegistry, InMemoryProcessedDepositTracker, SimulationClock,
    };
    use chrono::Duration;
    use rust_decimal_macros::dec;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    async fn setup() -> (
        ProcessDepositUseCase<
            SimulationClock,
            InMemoryAccountRepository,
            BroadcastEventPublisher,
            BlockchainAdapter,
            BlockchainAdapter,
            InMemoryDepositAddressRegistry,
            InMemoryProcessedDepositTracker,
        >,
        Arc<RwLock<BlockchainSimulator>>,
        Arc<SimulationClock>,
    ) {
        let clock = Arc::new(SimulationClock::new());
        let account_repo = Arc::new(InMemoryAccountRepository::new());
        let event_publisher = Arc::new(BroadcastEventPublisher::new(1000));

        let blockchain = Arc::new(RwLock::new(BlockchainSimulator::new(clock.now())));
        let blockchain_adapter = Arc::new(BlockchainAdapter::new(Arc::clone(&blockchain)));

        let address_registry = Arc::new(InMemoryDepositAddressRegistry::new());
        let processed_tracker = Arc::new(InMemoryProcessedDepositTracker::new());

        let use_case = ProcessDepositUseCase::new(
            Arc::clone(&clock),
            account_repo,
            event_publisher,
            Arc::clone(&blockchain_adapter),
            Arc::clone(&blockchain_adapter),
            address_registry,
            processed_tracker,
        );

        (use_case, blockchain, clock)
    }

    #[tokio::test]
    async fn test_generate_deposit_address() {
        let (use_case, _, _) = setup().await;

        let address = use_case
            .generate_deposit_address("user1", Network::Ethereum, Some("USDT".to_string()))
            .await
            .unwrap();

        assert!(address.address.starts_with("0x"));
        assert_eq!(address.owner_id, "user1");
    }

    #[tokio::test]
    async fn test_process_deposit_credits_account() {
        let (use_case, blockchain, clock) = setup().await;

        // Generate deposit address
        let address = use_case
            .generate_deposit_address("user1", Network::Ethereum, None)
            .await
            .unwrap();

        // Simulate incoming transaction
        {
            let mut bc = blockchain.write().await;
            bc.submit_transaction(
                &Network::Ethereum,
                "0xexternal",
                &address.address,
                "ETH",
                dec!(1.5),
                clock.now(),
            )
            .unwrap();

            // Advance time for finalization (150 seconds for Ethereum)
            clock.advance(Duration::seconds(200));
            bc.advance_time(clock.now());
        }

        // Process deposits
        let result = use_case.process_deposits().await.unwrap();

        assert_eq!(result.processed_count, 1);
        assert_eq!(result.credited_deposits[0].amount, dec!(1.5));
        assert_eq!(result.credited_deposits[0].asset, "ETH");

        // Verify account was credited
        let account = use_case.account_repo.get_by_owner("user1").await.unwrap();
        assert_eq!(account.balance("ETH").available, dec!(1.5));
    }

    #[tokio::test]
    async fn test_no_double_credit() {
        let (use_case, blockchain, clock) = setup().await;

        let address = use_case
            .generate_deposit_address("user1", Network::Ethereum, None)
            .await
            .unwrap();

        // Submit and finalize transaction
        {
            let mut bc = blockchain.write().await;
            bc.submit_transaction(
                &Network::Ethereum,
                "0xexternal",
                &address.address,
                "ETH",
                dec!(1.0),
                clock.now(),
            )
            .unwrap();

            clock.advance(Duration::seconds(200));
            bc.advance_time(clock.now());
        }

        // Process twice
        let result1 = use_case.process_deposits().await.unwrap();
        let result2 = use_case.process_deposits().await.unwrap();

        assert_eq!(result1.processed_count, 1);
        assert_eq!(result2.processed_count, 0); // Should not process again

        // Account should only have 1.0, not 2.0
        let account = use_case.account_repo.get_by_owner("user1").await.unwrap();
        assert_eq!(account.balance("ETH").available, dec!(1.0));
    }
}
