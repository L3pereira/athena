//! Integration test for cross-exchange crypto transfers.
//!
//! This test demonstrates the full flow of:
//! 1. Withdraw crypto from Exchange A (e.g., Binance)
//! 2. Transaction submitted to blockchain
//! 3. Transaction finalized after confirmations
//! 4. Exchange B (e.g., Coinbase) detects deposit and credits account
//!
//! This simulates an agent/bot moving funds between exchanges.

use chrono::Duration;
use exchange_sim::{
    AccountRepository, AddConfirmationCommand, BlockchainAdapter, BlockchainSimulator, Clock,
    ConfirmWithdrawalCommand, ControllableClock, Custodian, CustodianType, CustodianWriter,
    InMemoryDepositAddressRegistry, InMemoryProcessedDepositTracker, Network,
    ProcessDepositUseCase, ProcessWithdrawalCommand, ProcessWithdrawalUseCase,
    RequestWithdrawalCommand, RequestWithdrawalUseCase, SimulationClock, WithdrawalConfig,
    WithdrawalReader, WithdrawalStatus,
};
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Represents an exchange's infrastructure for the test
struct ExchangeContext {
    name: String,
    clock: Arc<SimulationClock>,
    account_repo: Arc<exchange_sim::InMemoryAccountRepository>,
    custodian_repo: Arc<exchange_sim::InMemoryCustodianRepository>,
    withdrawal_repo: Arc<exchange_sim::InMemoryWithdrawalRepository>,
    event_publisher: Arc<exchange_sim::BroadcastEventPublisher>,
}

impl ExchangeContext {
    fn new(name: &str, clock: Arc<SimulationClock>) -> Self {
        Self {
            name: name.to_string(),
            clock,
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

    async fn setup_withdrawal_custodian(
        &self,
        network: Network,
        asset: &str,
        fee: rust_decimal::Decimal,
    ) {
        let mut custodian = Custodian::new(
            format!("{} Hot Wallet", self.name),
            CustodianType::HotWallet,
            network.clone(),
        );

        let config = WithdrawalConfig::new(asset, network)
            .with_fee(fee)
            .with_min_amount(dec!(0.001)) // Low minimum for testing
            .with_max_amount(dec!(100000))
            .with_confirmations(12) // Ethereum confirmations
            .with_processing_time(30);

        custodian = custodian.with_withdrawal_config(config);
        custodian.deposit(asset, dec!(1000000)); // Hot wallet balance

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

    fn deposit_use_case(
        &self,
        blockchain: Arc<RwLock<BlockchainSimulator>>,
    ) -> ProcessDepositUseCase<
        SimulationClock,
        exchange_sim::InMemoryAccountRepository,
        exchange_sim::BroadcastEventPublisher,
        BlockchainAdapter,
        BlockchainAdapter,
        InMemoryDepositAddressRegistry,
        InMemoryProcessedDepositTracker,
    > {
        let blockchain_adapter = Arc::new(BlockchainAdapter::new(blockchain));
        let address_registry = Arc::new(InMemoryDepositAddressRegistry::new());
        let processed_tracker = Arc::new(InMemoryProcessedDepositTracker::new());

        ProcessDepositUseCase::new(
            Arc::clone(&self.clock),
            Arc::clone(&self.account_repo),
            Arc::clone(&self.event_publisher),
            Arc::clone(&blockchain_adapter),
            blockchain_adapter,
            address_registry,
            processed_tracker,
        )
    }
}

// ============================================================================
// CROSS-EXCHANGE TRANSFER TESTS
// ============================================================================

/// Test: Agent withdraws ETH from Exchange A, deposits to Exchange B via blockchain
#[tokio::test]
async fn test_cross_exchange_eth_transfer() {
    // Shared clock and blockchain for both exchanges
    let clock = Arc::new(SimulationClock::new());
    let blockchain = Arc::new(RwLock::new(BlockchainSimulator::new(clock.now())));

    // Setup Exchange A (source - e.g., "Binance")
    let exchange_a = ExchangeContext::new("ExchangeA", Arc::clone(&clock));
    exchange_a
        .setup_account_with_balance("agent_bot", "ETH", dec!(10.0))
        .await;
    exchange_a
        .setup_withdrawal_custodian(Network::Ethereum, "ETH", dec!(0.001))
        .await;

    // Setup Exchange B (destination - e.g., "Coinbase")
    let exchange_b = ExchangeContext::new("ExchangeB", Arc::clone(&clock));
    let deposit_use_case = exchange_b.deposit_use_case(Arc::clone(&blockchain));

    // Step 1: Generate deposit address on Exchange B for the agent
    let deposit_address = deposit_use_case
        .generate_deposit_address("agent_bot", Network::Ethereum, Some("ETH".to_string()))
        .await
        .expect("Should generate deposit address");

    println!(
        "Agent's deposit address on Exchange B: {}",
        deposit_address.address
    );

    // Step 2: Agent initiates withdrawal from Exchange A to deposit address on Exchange B
    let withdraw_use_case = exchange_a.request_withdrawal_use_case();
    let withdrawal_result = withdraw_use_case
        .execute(
            "agent_bot",
            RequestWithdrawalCommand {
                asset: "ETH".to_string(),
                amount: dec!(5.0),
                network: Network::Ethereum,
                destination_address: deposit_address.address.clone(),
                memo: None,
            },
        )
        .await
        .expect("Withdrawal request should succeed");

    let withdrawal_id = withdrawal_result.withdrawal.id;
    println!("Withdrawal requested: {:?}", withdrawal_id);

    // Verify Exchange A locked the funds
    let account_a = exchange_a
        .account_repo
        .get_by_owner("agent_bot")
        .await
        .unwrap();
    let balance_a = account_a.balance("ETH");
    assert!(balance_a.locked > dec!(0), "Funds should be locked");
    println!(
        "Exchange A balance - Available: {}, Locked: {}",
        balance_a.available, balance_a.locked
    );

    // Step 3: Exchange A processes the withdrawal (starts processing)
    let process_use_case = exchange_a.process_withdrawal_use_case();
    let process_result = process_use_case
        .start_processing(ProcessWithdrawalCommand { withdrawal_id })
        .await
        .expect("Should start processing");
    assert_eq!(process_result.status, WithdrawalStatus::Processing);

    // Step 4: Submit blockchain transaction
    // In real world, this would call blockchain API; we simulate by directly submitting to simulator
    let tx_hash = {
        let mut bc = blockchain.write().await;
        let tx_id = bc
            .submit_transaction(
                &Network::Ethereum,
                "exchange_a_hot_wallet",  // From Exchange A's hot wallet
                &deposit_address.address, // To agent's deposit address on Exchange B
                "ETH",
                dec!(5.0),
                clock.now(),
            )
            .expect("Transaction should submit");
        format!("{}", tx_id)
    };

    // Mark withdrawal as submitted with tx hash
    let submit_result = process_use_case
        .submit_transaction(ConfirmWithdrawalCommand {
            withdrawal_id,
            tx_hash: tx_hash.clone(),
        })
        .await
        .expect("Should submit transaction");
    assert_eq!(submit_result.status, WithdrawalStatus::AwaitingConfirmation);
    println!("Transaction submitted to blockchain: {}", tx_hash);

    // Step 5: Advance blockchain time and add confirmations
    // Ethereum needs 12 confirmations, each block is ~12 seconds
    for i in 1..=12 {
        clock.advance(Duration::seconds(12));
        {
            let mut bc = blockchain.write().await;
            bc.advance_time(clock.now());
        }

        let _ = process_use_case
            .add_confirmation(AddConfirmationCommand { withdrawal_id })
            .await;

        if i % 4 == 0 {
            println!("Confirmation {}/12 added", i);
        }
    }

    // Verify withdrawal completed on Exchange A
    let final_withdrawal = exchange_a
        .withdrawal_repo
        .get(&withdrawal_id)
        .await
        .expect("Withdrawal should exist");
    assert_eq!(final_withdrawal.status, WithdrawalStatus::Completed);
    println!(
        "Exchange A withdrawal completed: {:?}",
        final_withdrawal.status
    );

    // Step 6: Exchange B processes deposits (scans blockchain)
    // Need additional time for finalization on blockchain
    clock.advance(Duration::seconds(60));
    {
        let mut bc = blockchain.write().await;
        bc.advance_time(clock.now());
    }

    let deposit_result = deposit_use_case
        .process_deposits()
        .await
        .expect("Should process deposits");

    println!(
        "Exchange B processed {} deposits",
        deposit_result.processed_count
    );
    assert_eq!(
        deposit_result.processed_count, 1,
        "Should have processed 1 deposit"
    );

    let credited_deposit = &deposit_result.credited_deposits[0];
    assert_eq!(credited_deposit.amount, dec!(5.0));
    assert_eq!(credited_deposit.asset, "ETH");
    assert_eq!(credited_deposit.owner_id, "agent_bot");
    println!(
        "Deposit credited: {} {} to {}",
        credited_deposit.amount, credited_deposit.asset, credited_deposit.owner_id
    );

    // Step 7: Verify final balances
    // Exchange A: 10 ETH - 5 ETH - 0.001 fee = 4.999 ETH
    let final_account_a = exchange_a
        .account_repo
        .get_by_owner("agent_bot")
        .await
        .unwrap();
    let final_balance_a = final_account_a.balance("ETH");
    assert_eq!(
        final_balance_a.available,
        dec!(4.999),
        "Exchange A available balance"
    );
    assert_eq!(final_balance_a.locked, dec!(0), "Exchange A locked balance");
    println!(
        "Final Exchange A balance: {} ETH",
        final_balance_a.available
    );

    // Exchange B: 0 + 5 ETH = 5 ETH
    let final_account_b = exchange_b
        .account_repo
        .get_by_owner("agent_bot")
        .await
        .unwrap();
    let final_balance_b = final_account_b.balance("ETH");
    assert_eq!(
        final_balance_b.available,
        dec!(5.0),
        "Exchange B available balance"
    );
    println!(
        "Final Exchange B balance: {} ETH",
        final_balance_b.available
    );

    println!("\n=== Cross-Exchange Transfer Complete ===");
    println!("Agent transferred 5 ETH from Exchange A to Exchange B (fee: 0.001 ETH)");
}

/// Test: Deposit is not double-credited if process_deposits is called multiple times
#[tokio::test]
async fn test_no_double_credit_on_cross_exchange_deposit() {
    let clock = Arc::new(SimulationClock::new());
    let blockchain = Arc::new(RwLock::new(BlockchainSimulator::new(clock.now())));

    let exchange = ExchangeContext::new("TestExchange", Arc::clone(&clock));
    let deposit_use_case = exchange.deposit_use_case(Arc::clone(&blockchain));

    // Generate deposit address
    let deposit_address = deposit_use_case
        .generate_deposit_address("user1", Network::Ethereum, None)
        .await
        .expect("Should generate address");

    // Submit transaction directly to blockchain
    {
        let mut bc = blockchain.write().await;
        bc.submit_transaction(
            &Network::Ethereum,
            "external_wallet",
            &deposit_address.address,
            "ETH",
            dec!(2.5),
            clock.now(),
        )
        .expect("Transaction should submit");
    }

    // Advance time for finalization
    clock.advance(Duration::seconds(200));
    {
        let mut bc = blockchain.write().await;
        bc.advance_time(clock.now());
    }

    // Process deposits multiple times
    let result1 = deposit_use_case.process_deposits().await.unwrap();
    let result2 = deposit_use_case.process_deposits().await.unwrap();
    let result3 = deposit_use_case.process_deposits().await.unwrap();

    assert_eq!(result1.processed_count, 1, "First call should credit");
    assert_eq!(result2.processed_count, 0, "Second call should not credit");
    assert_eq!(result3.processed_count, 0, "Third call should not credit");

    // Verify account has correct balance (not 3x)
    let account = exchange.account_repo.get_by_owner("user1").await.unwrap();
    assert_eq!(
        account.balance("ETH").available,
        dec!(2.5),
        "Should only be credited once"
    );
}

/// Test: Multiple deposits from different sources
#[tokio::test]
async fn test_multiple_deposits_from_different_sources() {
    let clock = Arc::new(SimulationClock::new());
    let blockchain = Arc::new(RwLock::new(BlockchainSimulator::new(clock.now())));

    let exchange = ExchangeContext::new("TestExchange", Arc::clone(&clock));
    let deposit_use_case = exchange.deposit_use_case(Arc::clone(&blockchain));

    // Generate deposit address
    let deposit_address = deposit_use_case
        .generate_deposit_address("user1", Network::Ethereum, None)
        .await
        .expect("Should generate address");

    // Submit multiple transactions from different sources
    {
        let mut bc = blockchain.write().await;

        // Deposit 1: From wallet A
        bc.submit_transaction(
            &Network::Ethereum,
            "wallet_a",
            &deposit_address.address,
            "ETH",
            dec!(1.0),
            clock.now(),
        )
        .unwrap();

        // Deposit 2: From wallet B
        bc.submit_transaction(
            &Network::Ethereum,
            "wallet_b",
            &deposit_address.address,
            "ETH",
            dec!(2.0),
            clock.now(),
        )
        .unwrap();

        // Deposit 3: USDT from wallet C
        bc.submit_transaction(
            &Network::Ethereum,
            "wallet_c",
            &deposit_address.address,
            "USDT",
            dec!(1000),
            clock.now(),
        )
        .unwrap();
    }

    // Advance time for finalization
    clock.advance(Duration::seconds(200));
    {
        let mut bc = blockchain.write().await;
        bc.advance_time(clock.now());
    }

    // Process deposits
    let result = deposit_use_case.process_deposits().await.unwrap();

    assert_eq!(result.processed_count, 3, "Should process 3 deposits");

    // Verify account balances
    let account = exchange.account_repo.get_by_owner("user1").await.unwrap();
    assert_eq!(
        account.balance("ETH").available,
        dec!(3.0),
        "ETH: 1.0 + 2.0 = 3.0"
    );
    assert_eq!(account.balance("USDT").available, dec!(1000), "USDT: 1000");
}

/// Test: Unregistered deposit addresses don't get credited
#[tokio::test]
async fn test_deposit_to_unregistered_address_not_credited() {
    let clock = Arc::new(SimulationClock::new());
    let blockchain = Arc::new(RwLock::new(BlockchainSimulator::new(clock.now())));

    let exchange = ExchangeContext::new("TestExchange", Arc::clone(&clock));
    let deposit_use_case = exchange.deposit_use_case(Arc::clone(&blockchain));

    // Submit transaction to a random address (not registered with exchange)
    {
        let mut bc = blockchain.write().await;
        bc.submit_transaction(
            &Network::Ethereum,
            "external_wallet",
            "0xunregistered_address_1234567890abcdef", // Not a registered deposit address
            "ETH",
            dec!(100),
            clock.now(),
        )
        .unwrap();
    }

    // Advance time
    clock.advance(Duration::seconds(200));
    {
        let mut bc = blockchain.write().await;
        bc.advance_time(clock.now());
    }

    // Process deposits
    let result = deposit_use_case.process_deposits().await.unwrap();

    assert_eq!(
        result.processed_count, 0,
        "Unregistered address should not be credited"
    );
}
