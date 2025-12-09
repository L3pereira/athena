//! Integration tests for custodian and withdrawal functionality
//!
//! Tests cover:
//! - Custodian setup and management
//! - Withdrawal request flow
//! - Withdrawal processing lifecycle
//! - Balance locking and unlocking

use exchange_sim::{
    AccountRepository, AddConfirmationCommand, ConfirmWithdrawalCommand, Custodian,
    CustodianReader, CustodianType, CustodianWriter, FailWithdrawalCommand, Network,
    ProcessWithdrawalCommand, ProcessWithdrawalUseCase, RequestWithdrawalCommand,
    RequestWithdrawalUseCase, SimulationClock, WithdrawalConfig, WithdrawalReader,
    WithdrawalStatus,
};
use rust_decimal_macros::dec;
use std::sync::Arc;

/// Setup helper for withdrawal tests
struct WithdrawalTestContext {
    clock: Arc<SimulationClock>,
    account_repo: Arc<exchange_sim::InMemoryAccountRepository>,
    custodian_repo: Arc<exchange_sim::InMemoryCustodianRepository>,
    withdrawal_repo: Arc<exchange_sim::InMemoryWithdrawalRepository>,
    event_publisher: Arc<exchange_sim::BroadcastEventPublisher>,
}

impl WithdrawalTestContext {
    fn new() -> Self {
        Self {
            clock: Arc::new(SimulationClock::fixed()),
            account_repo: Arc::new(exchange_sim::InMemoryAccountRepository::new()),
            custodian_repo: Arc::new(exchange_sim::InMemoryCustodianRepository::new()),
            withdrawal_repo: Arc::new(exchange_sim::InMemoryWithdrawalRepository::new()),
            event_publisher: Arc::new(exchange_sim::BroadcastEventPublisher::new(1000)),
        }
    }

    async fn setup_account_with_balance(
        &self,
        owner: &str,
        asset: &str,
        amount: rust_decimal::Decimal,
    ) {
        let mut account = self.account_repo.get_or_create(owner).await;
        account.deposit(asset, amount);
        self.account_repo.save(account).await;
    }

    async fn setup_custodian(&self, network: Network, assets: Vec<(&str, rust_decimal::Decimal)>) {
        let mut custodian =
            Custodian::new("Test Wallet", CustodianType::HotWallet, network.clone());

        for (asset, fee) in assets {
            let config = WithdrawalConfig::new(asset, network.clone())
                .with_fee(fee)
                .with_min_amount(dec!(10))
                .with_max_amount(dec!(100000))
                .with_confirmations(6)
                .with_processing_time(60);
            custodian = custodian.with_withdrawal_config(config);
            // Add sufficient balance to the custodian for withdrawals
            custodian.deposit(asset, dec!(1000000));
        }

        self.custodian_repo.save(custodian).await;
    }

    fn request_withdrawal_use_case(
        &self,
    ) -> RequestWithdrawalUseCase<
        SimulationClock,
        exchange_sim::InMemoryAccountRepository,
        exchange_sim::InMemoryWithdrawalRepository,
        exchange_sim::InMemoryCustodianRepository,
        exchange_sim::BroadcastEventPublisher,
    > {
        RequestWithdrawalUseCase::new(
            Arc::clone(&self.clock),
            Arc::clone(&self.account_repo),
            Arc::clone(&self.withdrawal_repo),
            Arc::clone(&self.custodian_repo),
            Arc::clone(&self.event_publisher),
        )
    }

    fn process_withdrawal_use_case(
        &self,
    ) -> ProcessWithdrawalUseCase<
        SimulationClock,
        exchange_sim::InMemoryAccountRepository,
        exchange_sim::InMemoryWithdrawalRepository,
        exchange_sim::InMemoryCustodianRepository,
        exchange_sim::BroadcastEventPublisher,
    > {
        ProcessWithdrawalUseCase::new(
            Arc::clone(&self.clock),
            Arc::clone(&self.account_repo),
            Arc::clone(&self.withdrawal_repo),
            Arc::clone(&self.custodian_repo),
            Arc::clone(&self.event_publisher),
        )
    }
}

// ============================================================================
// CUSTODIAN TESTS
// ============================================================================

mod custodian_tests {
    use super::*;

    #[tokio::test]
    async fn test_custodian_creation_and_retrieval() {
        let ctx = WithdrawalTestContext::new();

        // Create custodian with withdrawal configs
        ctx.setup_custodian(
            Network::Ethereum,
            vec![("USDT", dec!(5)), ("ETH", dec!(0.001))],
        )
        .await;

        // Retrieve by network
        let custodians = ctx.custodian_repo.get_by_network(&Network::Ethereum).await;
        assert_eq!(custodians.len(), 1);

        let custodian = &custodians[0];
        assert_eq!(custodian.name, "Test Wallet");
        assert!(custodian.supports_withdrawal("USDT"));
        assert!(custodian.supports_withdrawal("ETH"));
        assert!(!custodian.supports_withdrawal("BTC"));
    }

    #[tokio::test]
    async fn test_custodian_supports_asset() {
        let ctx = WithdrawalTestContext::new();

        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;

        let supporting = ctx.custodian_repo.get_supporting_asset("USDT").await;
        assert_eq!(supporting.len(), 1);

        let not_supporting = ctx.custodian_repo.get_supporting_asset("BTC").await;
        assert_eq!(not_supporting.len(), 0);
    }

    #[tokio::test]
    async fn test_multiple_custodians_different_networks() {
        let ctx = WithdrawalTestContext::new();

        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;
        ctx.setup_custodian(Network::Bitcoin, vec![("BTC", dec!(0.0001))])
            .await;

        let eth_custodians = ctx.custodian_repo.get_by_network(&Network::Ethereum).await;
        assert_eq!(eth_custodians.len(), 1);

        let btc_custodians = ctx.custodian_repo.get_by_network(&Network::Bitcoin).await;
        assert_eq!(btc_custodians.len(), 1);

        let all_active = ctx.custodian_repo.get_active().await;
        assert_eq!(all_active.len(), 2);
    }
}

// ============================================================================
// WITHDRAWAL REQUEST TESTS
// ============================================================================

mod withdrawal_request_tests {
    use super::*;

    #[tokio::test]
    async fn test_request_withdrawal_success() {
        let ctx = WithdrawalTestContext::new();

        // Setup: account with 1000 USDT, custodian supporting USDT
        ctx.setup_account_with_balance("trader1", "USDT", dec!(1000))
            .await;
        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;

        let use_case = ctx.request_withdrawal_use_case();

        let command = RequestWithdrawalCommand {
            asset: "USDT".to_string(),
            amount: dec!(100),
            network: Network::Ethereum,
            destination_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            memo: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.withdrawal.amount, dec!(100));
        assert_eq!(result.fee, dec!(5));
        assert_eq!(result.withdrawal.status, WithdrawalStatus::Pending);

        // Check funds are locked
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        let balance = account.balance("USDT");
        assert_eq!(balance.available, dec!(895)); // 1000 - 100 - 5 = 895
        assert_eq!(balance.locked, dec!(105)); // 100 + 5 fee
    }

    #[tokio::test]
    async fn test_request_withdrawal_insufficient_balance() {
        let ctx = WithdrawalTestContext::new();

        ctx.setup_account_with_balance("trader1", "USDT", dec!(50))
            .await;
        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;

        let use_case = ctx.request_withdrawal_use_case();

        let command = RequestWithdrawalCommand {
            asset: "USDT".to_string(),
            amount: dec!(100), // More than balance
            network: Network::Ethereum,
            destination_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            memo: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_request_withdrawal_no_custodian() {
        let ctx = WithdrawalTestContext::new();

        ctx.setup_account_with_balance("trader1", "USDT", dec!(1000))
            .await;
        // No custodian setup

        let use_case = ctx.request_withdrawal_use_case();

        let command = RequestWithdrawalCommand {
            asset: "USDT".to_string(),
            amount: dec!(100),
            network: Network::Ethereum,
            destination_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            memo: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_request_withdrawal_below_minimum() {
        let ctx = WithdrawalTestContext::new();

        ctx.setup_account_with_balance("trader1", "USDT", dec!(1000))
            .await;
        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;

        let use_case = ctx.request_withdrawal_use_case();

        let command = RequestWithdrawalCommand {
            asset: "USDT".to_string(),
            amount: dec!(5), // Below minimum of 10
            network: Network::Ethereum,
            destination_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            memo: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
    }
}

// ============================================================================
// WITHDRAWAL PROCESSING TESTS
// ============================================================================

mod withdrawal_processing_tests {
    use super::*;

    #[tokio::test]
    async fn test_withdrawal_full_lifecycle() {
        let ctx = WithdrawalTestContext::new();

        // Setup
        ctx.setup_account_with_balance("trader1", "USDT", dec!(1000))
            .await;
        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;

        let request_use_case = ctx.request_withdrawal_use_case();
        let process_use_case = ctx.process_withdrawal_use_case();

        // Step 1: Request withdrawal
        let command = RequestWithdrawalCommand {
            asset: "USDT".to_string(),
            amount: dec!(100),
            network: Network::Ethereum,
            destination_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            memo: None,
        };

        let request_result = request_use_case.execute("trader1", command).await.unwrap();
        let withdrawal_id = request_result.withdrawal.id;
        assert_eq!(request_result.withdrawal.status, WithdrawalStatus::Pending);

        // Step 2: Start processing
        let process_result = process_use_case
            .start_processing(ProcessWithdrawalCommand { withdrawal_id })
            .await
            .unwrap();
        assert_eq!(process_result.status, WithdrawalStatus::Processing);

        // Step 3: Submit transaction
        let submit_result = process_use_case
            .submit_transaction(ConfirmWithdrawalCommand {
                withdrawal_id,
                tx_hash: "0xabcdef123456...".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(submit_result.status, WithdrawalStatus::AwaitingConfirmation);

        // Step 4: Add confirmations until complete
        for _ in 0..6 {
            let _ = process_use_case
                .add_confirmation(AddConfirmationCommand { withdrawal_id })
                .await
                .unwrap();
        }

        // Verify completed
        let withdrawal = ctx.withdrawal_repo.get(&withdrawal_id).await.unwrap();
        assert_eq!(withdrawal.status, WithdrawalStatus::Completed);

        // Verify account balance updated
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        let balance = account.balance("USDT");
        assert_eq!(balance.available, dec!(895)); // 1000 - 105
        assert_eq!(balance.locked, dec!(0));
    }

    #[tokio::test]
    async fn test_withdrawal_cancellation() {
        let ctx = WithdrawalTestContext::new();

        ctx.setup_account_with_balance("trader1", "USDT", dec!(1000))
            .await;
        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;

        let request_use_case = ctx.request_withdrawal_use_case();
        let process_use_case = ctx.process_withdrawal_use_case();

        // Request withdrawal
        let command = RequestWithdrawalCommand {
            asset: "USDT".to_string(),
            amount: dec!(100),
            network: Network::Ethereum,
            destination_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            memo: None,
        };

        let request_result = request_use_case.execute("trader1", command).await.unwrap();
        let withdrawal_id = request_result.withdrawal.id;

        // Cancel withdrawal
        let cancel_result = process_use_case.cancel(&withdrawal_id).await.unwrap();
        assert_eq!(cancel_result.status, WithdrawalStatus::Cancelled);

        // Verify funds returned
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        let balance = account.balance("USDT");
        assert_eq!(balance.available, dec!(1000)); // Full balance restored
        assert_eq!(balance.locked, dec!(0));
    }

    #[tokio::test]
    async fn test_withdrawal_failure() {
        let ctx = WithdrawalTestContext::new();

        ctx.setup_account_with_balance("trader1", "USDT", dec!(1000))
            .await;
        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;

        let request_use_case = ctx.request_withdrawal_use_case();
        let process_use_case = ctx.process_withdrawal_use_case();

        // Request and start processing
        let command = RequestWithdrawalCommand {
            asset: "USDT".to_string(),
            amount: dec!(100),
            network: Network::Ethereum,
            destination_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            memo: None,
        };

        let request_result = request_use_case.execute("trader1", command).await.unwrap();
        let withdrawal_id = request_result.withdrawal.id;

        process_use_case
            .start_processing(ProcessWithdrawalCommand { withdrawal_id })
            .await
            .unwrap();

        // Fail the withdrawal
        let fail_result = process_use_case
            .fail(FailWithdrawalCommand {
                withdrawal_id,
                reason: "Network congestion".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(fail_result.status, WithdrawalStatus::Failed);

        // Verify funds returned
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        let balance = account.balance("USDT");
        assert_eq!(balance.available, dec!(1000));
        assert_eq!(balance.locked, dec!(0));
    }

    #[tokio::test]
    async fn test_get_pending_withdrawals() {
        let ctx = WithdrawalTestContext::new();

        ctx.setup_account_with_balance("trader1", "USDT", dec!(5000))
            .await;
        ctx.setup_custodian(Network::Ethereum, vec![("USDT", dec!(5))])
            .await;

        let request_use_case = ctx.request_withdrawal_use_case();

        // Create multiple withdrawals
        for i in 0..3 {
            let command = RequestWithdrawalCommand {
                asset: "USDT".to_string(),
                amount: dec!(100) + rust_decimal::Decimal::from(i),
                network: Network::Ethereum,
                destination_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
                memo: None,
            };
            request_use_case.execute("trader1", command).await.unwrap();
        }

        // Check pending withdrawals
        let pending = ctx.withdrawal_repo.get_pending().await;
        assert_eq!(pending.len(), 3);
    }
}
