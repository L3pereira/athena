//! Integration tests for DEX (Decentralized Exchange) functionality
//!
//! Tests cover:
//! - Liquidity pool creation and management
//! - Token swaps with AMM (Automated Market Maker)
//! - Adding and removing liquidity
//! - LP token management
//! - Slippage protection
//! - Impermanent loss calculation

use exchange_sim::{
    AccountRepository, AddLiquidityCommand, LiquidityPool, LiquidityUseCase, PoolReader,
    PoolWriter, RemoveLiquidityCommand, SimulationClock, SwapCommand, SwapUseCase, Value,
};
use std::sync::Arc;

/// Setup helper for DEX tests
struct DexTestContext {
    clock: Arc<SimulationClock>,
    account_repo: Arc<exchange_sim::InMemoryAccountRepository>,
    pool_repo: Arc<exchange_sim::InMemoryPoolRepository>,
    event_publisher: Arc<exchange_sim::BroadcastEventPublisher>,
}

impl DexTestContext {
    fn new() -> Self {
        Self {
            clock: Arc::new(SimulationClock::fixed()),
            account_repo: Arc::new(exchange_sim::InMemoryAccountRepository::new()),
            pool_repo: Arc::new(exchange_sim::InMemoryPoolRepository::new()),
            event_publisher: Arc::new(exchange_sim::BroadcastEventPublisher::new(1000)),
        }
    }

    async fn setup_account_with_balances(&self, owner: &str, balances: Vec<(&str, Value)>) {
        let mut account = self.account_repo.get_or_create(owner).await;
        for (asset, amount) in balances {
            account.deposit(asset, amount);
        }
        self.account_repo.save(account).await;
    }

    async fn setup_pool_with_liquidity(
        &self,
        token_a: &str,
        token_b: &str,
        reserve_a: Value,
        reserve_b: Value,
    ) -> LiquidityPool {
        let mut pool = LiquidityPool::new(token_a, token_b);
        pool.reserve_a = reserve_a;
        pool.reserve_b = reserve_b;
        // LP supply = sqrt(reserve_a * reserve_b)
        let ra = reserve_a.to_f64();
        let rb = reserve_b.to_f64();
        pool.lp_token_supply = Value::from_f64((ra * rb).sqrt());
        self.pool_repo.save(pool.clone()).await;
        pool
    }

    fn swap_use_case(
        &self,
    ) -> SwapUseCase<
        SimulationClock,
        exchange_sim::InMemoryAccountRepository,
        exchange_sim::InMemoryPoolRepository,
        exchange_sim::BroadcastEventPublisher,
    > {
        SwapUseCase::new(
            Arc::clone(&self.clock),
            Arc::clone(&self.account_repo),
            Arc::clone(&self.pool_repo),
            Arc::clone(&self.event_publisher),
        )
    }

    fn liquidity_use_case(
        &self,
    ) -> LiquidityUseCase<
        SimulationClock,
        exchange_sim::InMemoryAccountRepository,
        exchange_sim::InMemoryPoolRepository,
        exchange_sim::BroadcastEventPublisher,
    > {
        LiquidityUseCase::new(
            Arc::clone(&self.clock),
            Arc::clone(&self.account_repo),
            Arc::clone(&self.pool_repo),
            Arc::clone(&self.event_publisher),
        )
    }
}

// ============================================================================
// LIQUIDITY POOL TESTS
// ============================================================================

mod liquidity_pool_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_pool_with_initial_liquidity() {
        let ctx = DexTestContext::new();

        // Setup: trader with tokens
        ctx.setup_account_with_balances(
            "lp1",
            vec![
                ("USDT", Value::from_int(10000)),
                ("ETH", Value::from_int(10)),
            ],
        )
        .await;

        let use_case = ctx.liquidity_use_case();

        // Add initial liquidity
        let command = AddLiquidityCommand {
            token_a: "USDT".to_string(),
            token_b: "ETH".to_string(),
            amount_a: Value::from_int(5000),
            amount_b: Value::from_int(5),
            min_lp_tokens: Value::ZERO,
        };

        let result = use_case.add_liquidity("lp1", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.result.lp_tokens.raw() > 0);
        assert_eq!(result.result.share_of_pool_bps, 10000); // 100% share for first LP (10000 bps)

        // Verify pool created
        let pool = ctx.pool_repo.get(&result.pool_id).await.unwrap();
        assert_eq!(pool.reserve_a, Value::from_int(5000));
        assert_eq!(pool.reserve_b, Value::from_int(5));

        // Verify account balances updated
        let account = ctx.account_repo.get_by_owner("lp1").await.unwrap();
        assert_eq!(account.balance("USDT").available, Value::from_int(5000));
        assert_eq!(account.balance("ETH").available, Value::from_int(5));
        assert!(account.balance(&pool.lp_token_symbol).available.raw() > 0);
    }

    #[tokio::test]
    async fn test_add_liquidity_to_existing_pool() {
        let ctx = DexTestContext::new();

        // Setup pool with existing liquidity
        let pool = ctx
            .setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;

        // Setup second LP
        ctx.setup_account_with_balances(
            "lp2",
            vec![("USDT", Value::from_int(5000)), ("ETH", Value::from_int(5))],
        )
        .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity proportionally
        let command = AddLiquidityCommand {
            token_a: "USDT".to_string(),
            token_b: "ETH".to_string(),
            amount_a: Value::from_int(5000),
            amount_b: Value::from_int(5),
            min_lp_tokens: Value::ZERO,
        };

        let result = use_case.add_liquidity("lp2", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // Should get roughly 1/3 of pool (5000 added to 10000 existing)
        let share_bps = result.result.share_of_pool_bps;
        assert!(share_bps > 3000 && share_bps < 3500); // 30-35% as bps

        // Verify pool reserves updated
        let updated_pool = ctx.pool_repo.get(&pool.id).await.unwrap();
        assert_eq!(updated_pool.reserve_a, Value::from_int(15000));
        assert_eq!(updated_pool.reserve_b, Value::from_int(15));
    }

    #[tokio::test]
    async fn test_remove_liquidity() {
        let ctx = DexTestContext::new();

        // Setup: create pool with liquidity from LP
        ctx.setup_account_with_balances(
            "lp1",
            vec![
                ("USDT", Value::from_int(10000)),
                ("ETH", Value::from_int(10)),
            ],
        )
        .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity
        let add_command = AddLiquidityCommand {
            token_a: "USDT".to_string(),
            token_b: "ETH".to_string(),
            amount_a: Value::from_int(5000),
            amount_b: Value::from_int(5),
            min_lp_tokens: Value::ZERO,
        };

        let add_result = use_case.add_liquidity("lp1", add_command).await.unwrap();
        let pool_id = add_result.pool_id;
        let lp_tokens = add_result.result.lp_tokens;

        // Remove half of liquidity
        let half_lp = Value::from_raw(lp_tokens.raw() / 2);
        let remove_command = RemoveLiquidityCommand {
            pool_id,
            lp_tokens: half_lp,
            min_amount_a: Value::ZERO,
            min_amount_b: Value::ZERO,
        };

        let remove_result = use_case.remove_liquidity("lp1", remove_command).await;
        assert!(remove_result.is_ok());

        let remove_result = remove_result.unwrap();
        // Should get back roughly half of tokens
        let amount_a = remove_result.result.amount_a.to_f64();
        let amount_b = remove_result.result.amount_b.to_f64();
        assert!(amount_a > 2400.0 && amount_a < 2600.0);
        assert!(amount_b > 2.4 && amount_b < 2.6);

        // Verify account received tokens back
        let account = ctx.account_repo.get_by_owner("lp1").await.unwrap();
        assert!(account.balance("USDT").available.to_f64() > 7000.0);
        assert!(account.balance("ETH").available.to_f64() > 7.0);
    }

    #[tokio::test]
    async fn test_insufficient_lp_tokens() {
        let ctx = DexTestContext::new();

        ctx.setup_account_with_balances(
            "lp1",
            vec![("USDT", Value::from_int(5000)), ("ETH", Value::from_int(5))],
        )
        .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity
        let add_command = AddLiquidityCommand {
            token_a: "USDT".to_string(),
            token_b: "ETH".to_string(),
            amount_a: Value::from_int(5000),
            amount_b: Value::from_int(5),
            min_lp_tokens: Value::ZERO,
        };

        let add_result = use_case.add_liquidity("lp1", add_command).await.unwrap();
        let lp_tokens = add_result.result.lp_tokens;

        // Try to remove more LP tokens than owned
        let double_lp = Value::from_raw(lp_tokens.raw() * 2);
        let remove_command = RemoveLiquidityCommand {
            pool_id: add_result.pool_id,
            lp_tokens: double_lp,
            min_amount_a: Value::ZERO,
            min_amount_b: Value::ZERO,
        };

        let result = use_case.remove_liquidity("lp1", remove_command).await;
        assert!(result.is_err());
    }
}

// ============================================================================
// SWAP TESTS
// ============================================================================

mod swap_tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_swap() {
        let ctx = DexTestContext::new();

        // Setup pool: 10000 USDT / 10 ETH = 1000 USDT/ETH
        ctx.setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;

        // Setup trader
        ctx.setup_account_with_balances("trader1", vec![("USDT", Value::from_int(1000))])
            .await;

        let use_case = ctx.swap_use_case();

        // Swap 1000 USDT for ETH
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: Value::from_int(1000),
            min_amount_out: Value::ZERO,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // With constant product formula, should get slightly less than 1 ETH due to price impact
        let amount_out = result.swap_result.amount_out.to_f64();
        assert!(amount_out > 0.9);
        assert!(amount_out < 1.0);

        // Verify account balances
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        assert_eq!(account.balance("USDT").available, Value::ZERO);
        assert!(account.balance("ETH").available.to_f64() > 0.9);
    }

    #[tokio::test]
    async fn test_swap_reverse_direction() {
        let ctx = DexTestContext::new();

        // Setup pool
        ctx.setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;

        // Setup trader with ETH
        ctx.setup_account_with_balances("trader1", vec![("ETH", Value::from_int(1))])
            .await;

        let use_case = ctx.swap_use_case();

        // Swap 1 ETH for USDT
        let command = SwapCommand {
            token_in: "ETH".to_string(),
            token_out: "USDT".to_string(),
            amount_in: Value::from_int(1),
            min_amount_out: Value::ZERO,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // Should get roughly 900 USDT (less due to price impact)
        let amount_out = result.swap_result.amount_out.to_f64();
        assert!(amount_out > 800.0);
        assert!(amount_out < 1000.0);

        // Verify account
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        assert_eq!(account.balance("ETH").available, Value::ZERO);
        assert!(account.balance("USDT").available.to_f64() > 800.0);
    }

    #[tokio::test]
    async fn test_swap_slippage_protection() {
        let ctx = DexTestContext::new();

        ctx.setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;

        ctx.setup_account_with_balances("trader1", vec![("USDT", Value::from_int(1000))])
            .await;

        let use_case = ctx.swap_use_case();

        // Swap with high minimum output expectation (should fail)
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: Value::from_int(1000),
            min_amount_out: Value::from_int(2), // Expecting 2 ETH, but will only get ~0.9
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());

        // Verify funds not deducted
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        assert_eq!(account.balance("USDT").available, Value::from_int(1000));
    }

    #[tokio::test]
    async fn test_swap_insufficient_balance() {
        let ctx = DexTestContext::new();

        ctx.setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;

        ctx.setup_account_with_balances("trader1", vec![("USDT", Value::from_int(100))])
            .await;

        let use_case = ctx.swap_use_case();

        // Try to swap more than balance
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: Value::from_int(1000), // Only have 100
            min_amount_out: Value::ZERO,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_swap_quote() {
        let ctx = DexTestContext::new();

        ctx.setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;

        let use_case = ctx.swap_use_case();

        // Get quote without executing
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: Value::from_int(1000),
            min_amount_out: Value::ZERO,
        };

        let quote = use_case.quote(&command).await;
        assert!(quote.is_ok());

        let quote = quote.unwrap();
        assert!(quote.amount_out.to_f64() > 0.9);
        assert!(quote.fee_amount.raw() > 0);
        assert!(quote.price_impact_bps > 0);
    }

    #[tokio::test]
    async fn test_large_swap_high_price_impact() {
        let ctx = DexTestContext::new();

        // Small pool
        ctx.setup_pool_with_liquidity("USDT", "ETH", Value::from_int(1000), Value::from_int(1))
            .await;

        ctx.setup_account_with_balances("trader1", vec![("USDT", Value::from_int(500))])
            .await;

        let use_case = ctx.swap_use_case();

        // Large swap relative to pool size
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: Value::from_int(500), // 50% of pool's USDT
            min_amount_out: Value::ZERO,
        };

        let result = use_case.execute("trader1", command).await.unwrap();

        // High price impact expected (> 30% = 3000 bps)
        assert!(result.swap_result.price_impact_bps > 3000);
    }
}

// ============================================================================
// POOL QUERY TESTS
// ============================================================================

mod pool_query_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_pools() {
        let ctx = DexTestContext::new();

        // Create multiple pools
        ctx.setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;
        ctx.setup_pool_with_liquidity("USDT", "BTC", Value::from_int(50000), Value::from_int(1))
            .await;
        ctx.setup_pool_with_liquidity("ETH", "BTC", Value::from_int(10), Value::from_f64(0.2))
            .await;

        let use_case = ctx.liquidity_use_case();

        let pools = use_case.get_pools().await;
        assert_eq!(pools.len(), 3);
    }

    #[tokio::test]
    async fn test_get_pool_by_tokens() {
        let ctx = DexTestContext::new();

        let pool = ctx
            .setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;

        // Should find regardless of token order
        let found1 = ctx.pool_repo.get_by_tokens("USDT", "ETH").await;
        assert!(found1.is_some());
        assert_eq!(found1.unwrap().id, pool.id);

        let found2 = ctx.pool_repo.get_by_tokens("ETH", "USDT").await;
        assert!(found2.is_some());
        assert_eq!(found2.unwrap().id, pool.id);
    }

    #[tokio::test]
    async fn test_get_lp_positions() {
        let ctx = DexTestContext::new();

        ctx.setup_account_with_balances(
            "lp1",
            vec![
                ("USDT", Value::from_int(20000)),
                ("ETH", Value::from_int(20)),
                ("BTC", Value::from_int(1)),
            ],
        )
        .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity to two pools
        use_case
            .add_liquidity(
                "lp1",
                AddLiquidityCommand {
                    token_a: "USDT".to_string(),
                    token_b: "ETH".to_string(),
                    amount_a: Value::from_int(5000),
                    amount_b: Value::from_int(5),
                    min_lp_tokens: Value::ZERO,
                },
            )
            .await
            .unwrap();

        use_case
            .add_liquidity(
                "lp1",
                AddLiquidityCommand {
                    token_a: "USDT".to_string(),
                    token_b: "BTC".to_string(),
                    amount_a: Value::from_int(10000),
                    amount_b: Value::from_f64(0.5),
                    min_lp_tokens: Value::ZERO,
                },
            )
            .await
            .unwrap();

        let positions = use_case.get_positions("lp1").await.unwrap();
        assert_eq!(positions.len(), 2);
    }
}

// ============================================================================
// IMPERMANENT LOSS TESTS
// ============================================================================

mod impermanent_loss_tests {
    use super::*;

    #[tokio::test]
    async fn test_impermanent_loss_calculation() {
        let ctx = DexTestContext::new();

        ctx.setup_account_with_balances(
            "lp1",
            vec![
                ("USDT", Value::from_int(10000)),
                ("ETH", Value::from_int(10)),
            ],
        )
        .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity at 1000 USDT/ETH price
        let result = use_case
            .add_liquidity(
                "lp1",
                AddLiquidityCommand {
                    token_a: "USDT".to_string(),
                    token_b: "ETH".to_string(),
                    amount_a: Value::from_int(5000),
                    amount_b: Value::from_int(5),
                    min_lp_tokens: Value::ZERO,
                },
            )
            .await
            .unwrap();

        // Simulate price change by modifying pool reserves (ETH price doubles)
        let mut pool = ctx.pool_repo.get(&result.pool_id).await.unwrap();
        pool.reserve_a = Value::from_int(7071); // sqrt(5000 * 10000) approximately
        pool.reserve_b = Value::from_f64(3.536); // To maintain k = 5000 * 5 = 25000
        ctx.pool_repo.save(pool).await;

        // Calculate IL
        let il = use_case
            .calculate_impermanent_loss("lp1", &result.pool_id)
            .await
            .unwrap();

        // Should show some impermanent loss (2x price change gives ~5.7% IL, but with
        // price changes it can vary; just verify it's positive and reasonable)
        assert!(il > 0);
        assert!(il < 5000); // IL should be less than 50% (5000 bps) for any reasonable scenario
    }
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

mod edge_cases {
    use super::*;

    #[tokio::test]
    async fn test_swap_on_nonexistent_pool() {
        let ctx = DexTestContext::new();

        ctx.setup_account_with_balances("trader1", vec![("USDT", Value::from_int(1000))])
            .await;

        let use_case = ctx.swap_use_case();

        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "NONEXISTENT".to_string(),
            amount_in: Value::from_int(100),
            min_amount_out: Value::ZERO,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiple_swaps_affect_price() {
        let ctx = DexTestContext::new();

        ctx.setup_pool_with_liquidity("USDT", "ETH", Value::from_int(10000), Value::from_int(10))
            .await;

        ctx.setup_account_with_balances("trader1", vec![("USDT", Value::from_int(3000))])
            .await;

        let use_case = ctx.swap_use_case();

        // First swap
        let result1 = use_case
            .execute(
                "trader1",
                SwapCommand {
                    token_in: "USDT".to_string(),
                    token_out: "ETH".to_string(),
                    amount_in: Value::from_int(1000),
                    min_amount_out: Value::ZERO,
                },
            )
            .await
            .unwrap();

        // Second swap (same size)
        let result2 = use_case
            .execute(
                "trader1",
                SwapCommand {
                    token_in: "USDT".to_string(),
                    token_out: "ETH".to_string(),
                    amount_in: Value::from_int(1000),
                    min_amount_out: Value::ZERO,
                },
            )
            .await
            .unwrap();

        // Second swap should get less ETH due to price movement
        assert!(result2.swap_result.amount_out.raw() < result1.swap_result.amount_out.raw());
    }

    #[tokio::test]
    async fn test_remove_all_liquidity() {
        let ctx = DexTestContext::new();

        ctx.setup_account_with_balances(
            "lp1",
            vec![("USDT", Value::from_int(5000)), ("ETH", Value::from_int(5))],
        )
        .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity
        let add_result = use_case
            .add_liquidity(
                "lp1",
                AddLiquidityCommand {
                    token_a: "USDT".to_string(),
                    token_b: "ETH".to_string(),
                    amount_a: Value::from_int(5000),
                    amount_b: Value::from_int(5),
                    min_lp_tokens: Value::ZERO,
                },
            )
            .await
            .unwrap();

        // Remove ALL liquidity
        let remove_result = use_case
            .remove_liquidity(
                "lp1",
                RemoveLiquidityCommand {
                    pool_id: add_result.pool_id,
                    lp_tokens: add_result.result.lp_tokens,
                    min_amount_a: Value::ZERO,
                    min_amount_b: Value::ZERO,
                },
            )
            .await
            .unwrap();

        assert_eq!(remove_result.remaining_lp_tokens, Value::ZERO);

        // Position should be deleted
        let positions = use_case.get_positions("lp1").await.unwrap();
        assert_eq!(positions.len(), 0);
    }
}
