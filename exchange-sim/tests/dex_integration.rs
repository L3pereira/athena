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
    PoolWriter, RemoveLiquidityCommand, SimulationClock, SwapCommand, SwapUseCase,
};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
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

    async fn setup_account_with_balances(&self, owner: &str, balances: Vec<(&str, Decimal)>) {
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
        reserve_a: Decimal,
        reserve_b: Decimal,
    ) -> LiquidityPool {
        let mut pool = LiquidityPool::new(token_a, token_b);
        pool.reserve_a = reserve_a;
        pool.reserve_b = reserve_b;
        pool.lp_token_supply = (reserve_a * reserve_b).sqrt().unwrap_or(dec!(0));
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
        ctx.setup_account_with_balances("lp1", vec![("USDT", dec!(10000)), ("ETH", dec!(10))])
            .await;

        let use_case = ctx.liquidity_use_case();

        // Add initial liquidity
        let command = AddLiquidityCommand {
            token_a: "USDT".to_string(),
            token_b: "ETH".to_string(),
            amount_a: dec!(5000),
            amount_b: dec!(5),
            min_lp_tokens: dec!(0),
        };

        let result = use_case.add_liquidity("lp1", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.result.lp_tokens > dec!(0));
        assert_eq!(result.result.share_of_pool, dec!(1)); // 100% share for first LP

        // Verify pool created
        let pool = ctx.pool_repo.get(&result.pool_id).await.unwrap();
        assert_eq!(pool.reserve_a, dec!(5000));
        assert_eq!(pool.reserve_b, dec!(5));

        // Verify account balances updated
        let account = ctx.account_repo.get_by_owner("lp1").await.unwrap();
        assert_eq!(account.balance("USDT").available, dec!(5000));
        assert_eq!(account.balance("ETH").available, dec!(5));
        assert!(account.balance(&pool.lp_token_symbol).available > dec!(0));
    }

    #[tokio::test]
    async fn test_add_liquidity_to_existing_pool() {
        let ctx = DexTestContext::new();

        // Setup pool with existing liquidity
        let pool = ctx
            .setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
            .await;

        // Setup second LP
        ctx.setup_account_with_balances("lp2", vec![("USDT", dec!(5000)), ("ETH", dec!(5))])
            .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity proportionally
        let command = AddLiquidityCommand {
            token_a: "USDT".to_string(),
            token_b: "ETH".to_string(),
            amount_a: dec!(5000),
            amount_b: dec!(5),
            min_lp_tokens: dec!(0),
        };

        let result = use_case.add_liquidity("lp2", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // Should get roughly 1/3 of pool (5000 added to 10000 existing)
        assert!(
            result.result.share_of_pool > dec!(0.3) && result.result.share_of_pool < dec!(0.35)
        );

        // Verify pool reserves updated
        let updated_pool = ctx.pool_repo.get(&pool.id).await.unwrap();
        assert_eq!(updated_pool.reserve_a, dec!(15000));
        assert_eq!(updated_pool.reserve_b, dec!(15));
    }

    #[tokio::test]
    async fn test_remove_liquidity() {
        let ctx = DexTestContext::new();

        // Setup: create pool with liquidity from LP
        ctx.setup_account_with_balances("lp1", vec![("USDT", dec!(10000)), ("ETH", dec!(10))])
            .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity
        let add_command = AddLiquidityCommand {
            token_a: "USDT".to_string(),
            token_b: "ETH".to_string(),
            amount_a: dec!(5000),
            amount_b: dec!(5),
            min_lp_tokens: dec!(0),
        };

        let add_result = use_case.add_liquidity("lp1", add_command).await.unwrap();
        let pool_id = add_result.pool_id;
        let lp_tokens = add_result.result.lp_tokens;

        // Remove half of liquidity
        let remove_command = RemoveLiquidityCommand {
            pool_id,
            lp_tokens: lp_tokens / dec!(2),
            min_amount_a: dec!(0),
            min_amount_b: dec!(0),
        };

        let remove_result = use_case.remove_liquidity("lp1", remove_command).await;
        assert!(remove_result.is_ok());

        let remove_result = remove_result.unwrap();
        // Should get back roughly half of tokens
        assert!(
            remove_result.result.amount_a > dec!(2400)
                && remove_result.result.amount_a < dec!(2600)
        );
        assert!(
            remove_result.result.amount_b > dec!(2.4) && remove_result.result.amount_b < dec!(2.6)
        );

        // Verify account received tokens back
        let account = ctx.account_repo.get_by_owner("lp1").await.unwrap();
        assert!(account.balance("USDT").available > dec!(7000));
        assert!(account.balance("ETH").available > dec!(7));
    }

    #[tokio::test]
    async fn test_insufficient_lp_tokens() {
        let ctx = DexTestContext::new();

        ctx.setup_account_with_balances("lp1", vec![("USDT", dec!(5000)), ("ETH", dec!(5))])
            .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity
        let add_command = AddLiquidityCommand {
            token_a: "USDT".to_string(),
            token_b: "ETH".to_string(),
            amount_a: dec!(5000),
            amount_b: dec!(5),
            min_lp_tokens: dec!(0),
        };

        let add_result = use_case.add_liquidity("lp1", add_command).await.unwrap();
        let lp_tokens = add_result.result.lp_tokens;

        // Try to remove more LP tokens than owned
        let remove_command = RemoveLiquidityCommand {
            pool_id: add_result.pool_id,
            lp_tokens: lp_tokens * dec!(2), // Double what we have
            min_amount_a: dec!(0),
            min_amount_b: dec!(0),
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
        ctx.setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
            .await;

        // Setup trader
        ctx.setup_account_with_balances("trader1", vec![("USDT", dec!(1000))])
            .await;

        let use_case = ctx.swap_use_case();

        // Swap 1000 USDT for ETH
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: dec!(1000),
            min_amount_out: dec!(0),
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // With constant product formula, should get slightly less than 1 ETH due to price impact
        assert!(result.swap_result.amount_out > dec!(0.9));
        assert!(result.swap_result.amount_out < dec!(1));

        // Verify account balances
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        assert_eq!(account.balance("USDT").available, dec!(0));
        assert!(account.balance("ETH").available > dec!(0.9));
    }

    #[tokio::test]
    async fn test_swap_reverse_direction() {
        let ctx = DexTestContext::new();

        // Setup pool
        ctx.setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
            .await;

        // Setup trader with ETH
        ctx.setup_account_with_balances("trader1", vec![("ETH", dec!(1))])
            .await;

        let use_case = ctx.swap_use_case();

        // Swap 1 ETH for USDT
        let command = SwapCommand {
            token_in: "ETH".to_string(),
            token_out: "USDT".to_string(),
            amount_in: dec!(1),
            min_amount_out: dec!(0),
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // Should get roughly 900 USDT (less due to price impact)
        assert!(result.swap_result.amount_out > dec!(800));
        assert!(result.swap_result.amount_out < dec!(1000));

        // Verify account
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        assert_eq!(account.balance("ETH").available, dec!(0));
        assert!(account.balance("USDT").available > dec!(800));
    }

    #[tokio::test]
    async fn test_swap_slippage_protection() {
        let ctx = DexTestContext::new();

        ctx.setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
            .await;

        ctx.setup_account_with_balances("trader1", vec![("USDT", dec!(1000))])
            .await;

        let use_case = ctx.swap_use_case();

        // Swap with high minimum output expectation (should fail)
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: dec!(1000),
            min_amount_out: dec!(2), // Expecting 2 ETH, but will only get ~0.9
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());

        // Verify funds not deducted
        let account = ctx.account_repo.get_by_owner("trader1").await.unwrap();
        assert_eq!(account.balance("USDT").available, dec!(1000));
    }

    #[tokio::test]
    async fn test_swap_insufficient_balance() {
        let ctx = DexTestContext::new();

        ctx.setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
            .await;

        ctx.setup_account_with_balances("trader1", vec![("USDT", dec!(100))])
            .await;

        let use_case = ctx.swap_use_case();

        // Try to swap more than balance
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: dec!(1000), // Only have 100
            min_amount_out: dec!(0),
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_swap_quote() {
        let ctx = DexTestContext::new();

        ctx.setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
            .await;

        let use_case = ctx.swap_use_case();

        // Get quote without executing
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: dec!(1000),
            min_amount_out: dec!(0),
        };

        let quote = use_case.quote(&command).await;
        assert!(quote.is_ok());

        let quote = quote.unwrap();
        assert!(quote.amount_out > dec!(0.9));
        assert!(quote.fee_amount > dec!(0));
        assert!(quote.price_impact > dec!(0));
    }

    #[tokio::test]
    async fn test_large_swap_high_price_impact() {
        let ctx = DexTestContext::new();

        // Small pool
        ctx.setup_pool_with_liquidity("USDT", "ETH", dec!(1000), dec!(1))
            .await;

        ctx.setup_account_with_balances("trader1", vec![("USDT", dec!(500))])
            .await;

        let use_case = ctx.swap_use_case();

        // Large swap relative to pool size
        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "ETH".to_string(),
            amount_in: dec!(500), // 50% of pool's USDT
            min_amount_out: dec!(0),
        };

        let result = use_case.execute("trader1", command).await.unwrap();

        // High price impact expected
        assert!(result.swap_result.price_impact > dec!(0.3)); // > 30% price impact
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
        ctx.setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
            .await;
        ctx.setup_pool_with_liquidity("USDT", "BTC", dec!(50000), dec!(1))
            .await;
        ctx.setup_pool_with_liquidity("ETH", "BTC", dec!(10), dec!(0.2))
            .await;

        let use_case = ctx.liquidity_use_case();

        let pools = use_case.get_pools().await;
        assert_eq!(pools.len(), 3);
    }

    #[tokio::test]
    async fn test_get_pool_by_tokens() {
        let ctx = DexTestContext::new();

        let pool = ctx
            .setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
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
            vec![("USDT", dec!(20000)), ("ETH", dec!(20)), ("BTC", dec!(1))],
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
                    amount_a: dec!(5000),
                    amount_b: dec!(5),
                    min_lp_tokens: dec!(0),
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
                    amount_a: dec!(10000),
                    amount_b: dec!(0.5),
                    min_lp_tokens: dec!(0),
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

        ctx.setup_account_with_balances("lp1", vec![("USDT", dec!(10000)), ("ETH", dec!(10))])
            .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity at 1000 USDT/ETH price
        let result = use_case
            .add_liquidity(
                "lp1",
                AddLiquidityCommand {
                    token_a: "USDT".to_string(),
                    token_b: "ETH".to_string(),
                    amount_a: dec!(5000),
                    amount_b: dec!(5),
                    min_lp_tokens: dec!(0),
                },
            )
            .await
            .unwrap();

        // Simulate price change by modifying pool reserves (ETH price doubles)
        let mut pool = ctx.pool_repo.get(&result.pool_id).await.unwrap();
        pool.reserve_a = dec!(7071); // sqrt(5000 * 10000) approximately
        pool.reserve_b = dec!(3.536); // To maintain k = 5000 * 5 = 25000
        ctx.pool_repo.save(pool).await;

        // Calculate IL
        let il = use_case
            .calculate_impermanent_loss("lp1", &result.pool_id)
            .await
            .unwrap();

        // Should show some impermanent loss (2x price change gives ~5.7% IL, but with
        // price changes it can vary; just verify it's positive and reasonable)
        assert!(il > dec!(0));
        assert!(il < dec!(0.5)); // IL should be less than 50% for any reasonable scenario
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

        ctx.setup_account_with_balances("trader1", vec![("USDT", dec!(1000))])
            .await;

        let use_case = ctx.swap_use_case();

        let command = SwapCommand {
            token_in: "USDT".to_string(),
            token_out: "NONEXISTENT".to_string(),
            amount_in: dec!(100),
            min_amount_out: dec!(0),
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiple_swaps_affect_price() {
        let ctx = DexTestContext::new();

        ctx.setup_pool_with_liquidity("USDT", "ETH", dec!(10000), dec!(10))
            .await;

        ctx.setup_account_with_balances("trader1", vec![("USDT", dec!(3000))])
            .await;

        let use_case = ctx.swap_use_case();

        // First swap
        let result1 = use_case
            .execute(
                "trader1",
                SwapCommand {
                    token_in: "USDT".to_string(),
                    token_out: "ETH".to_string(),
                    amount_in: dec!(1000),
                    min_amount_out: dec!(0),
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
                    amount_in: dec!(1000),
                    min_amount_out: dec!(0),
                },
            )
            .await
            .unwrap();

        // Second swap should get less ETH due to price movement
        assert!(result2.swap_result.amount_out < result1.swap_result.amount_out);
    }

    #[tokio::test]
    async fn test_remove_all_liquidity() {
        let ctx = DexTestContext::new();

        ctx.setup_account_with_balances("lp1", vec![("USDT", dec!(5000)), ("ETH", dec!(5))])
            .await;

        let use_case = ctx.liquidity_use_case();

        // Add liquidity
        let add_result = use_case
            .add_liquidity(
                "lp1",
                AddLiquidityCommand {
                    token_a: "USDT".to_string(),
                    token_b: "ETH".to_string(),
                    amount_a: dec!(5000),
                    amount_b: dec!(5),
                    min_lp_tokens: dec!(0),
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
                    min_amount_a: dec!(0),
                    min_amount_b: dec!(0),
                },
            )
            .await
            .unwrap();

        assert_eq!(remove_result.remaining_lp_tokens, dec!(0));

        // Position should be deleted
        let positions = use_case.get_positions("lp1").await.unwrap();
        assert_eq!(positions.len(), 0);
    }
}
